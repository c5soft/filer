use crate::base16::base16_encode;
use crate::fileutil;
use crate::json_helper::JsonHelper;
use anyhow::{anyhow, Result};
use blake3::Hasher;
use byte_unit::Byte;
use fileutil::{calc_parts, kill_running_exe, PartData, EOL, MAX_SPLIT_PARTS};
use reqwest::{Response, StatusCode};
use serde_json::{json, Value};
use std::io::SeekFrom;
use std::path::Path;
use tokio::fs::{self, DirBuilder, File};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::task;

pub fn base_url(config: &Value) -> String {
    let server = config["server"].str("192.168.100.4");
    let port = config["port"].u64(9191u64);
    let is_https = config["is_https"].bool(false);
    format!(
        "{}://{}:{}/api/download/",
        if is_https { "https" } else { "http" },
        server,
        port
    )
}
pub async fn request(base_url: &str, params: &Value) -> Result<Response> {
    //use tracing::debug;
    let params = base16_encode(&format!("{}", params))?;
    let url = String::from(base_url) + &params;
    //debug!("get {}", url);
    reqwest::get(url)
        .await
        .map_err(|e| anyhow!("download::request error {:?}", e))
}

pub async fn get_full_of_file(base_url: &str, catalog: &str, file: &str) -> Result<PartData> {
    get_part_of_file(base_url, catalog, file, 0, 0).await
}

pub async fn get_part_of_file(
    base_url: &str,
    catalog: &str,
    file: &str,
    skip: u64,
    take: u64,
) -> Result<PartData> {
    let params = if take == 0 {
        json!({"catalog":catalog,"file":file})
    } else {
        json!({"catalog":catalog,"file":file,"skip":skip,"take":take})
    };
    let response = request(base_url, &params).await?;
    if response.status() == StatusCode::OK {
        let headers = response.headers();
        let skip_val = headers.get("x-skip");
        let skip = if skip_val.is_none() {
            skip
        } else {
            u64::from_str_radix(skip_val.unwrap().to_str()?, 10)?
        };
        let take_val = headers.get("x-take");
        let take = if take_val.is_none() {
            take
        } else {
            u64::from_str_radix(take_val.unwrap().to_str()?, 10)?
        };
        let bytes = response.bytes().await?.to_vec();
        Ok((skip, take, bytes))
    } else if response.status() == StatusCode::NOT_ACCEPTABLE
        && response.headers().contains_key("x-body-is-error")
    {
        let msg = response.bytes().await?;
        let msg: String = String::from_utf8(msg.to_vec())?;
        Err(anyhow!("download files fail: {}", msg))
    } else {
        Err(anyhow!("download files fail: unkown reason {:?}",response.status()))
    }
}
//return (digest_calc,file_size_calc,parts,part_size,from_local)
async fn download_file(
    base_url: &str,
    catalog: &str,
    path: &str,
    file_name: &str,
    file_size: u64,
    part_size: u64,
    digest: &str,
    source_file_name: &str,
    from_local: bool,
) -> Result<(String, u64, u64, u64, bool)> {
    let local_source_file_name = path.to_string() + "/" + source_file_name;
    let source_file_name = String::from(source_file_name);
    let target_file_name = if file_name.to_lowercase().ends_with("filer.exe") {
        String::from(path) + "/" + file_name + ".new"
    } else {
        String::from(path) + "/" + file_name
    };
    let (parts, part_size) = calc_parts(file_size, part_size, MAX_SPLIT_PARTS);
    // println!(
    //     ">>writing {} with size {},splited {} parts*{} ...",
    //     target_file_name, file_size, parts, part_size
    // );
    let target_file_folder = Path::new(&target_file_name)
        .parent()
        .ok_or(anyhow!("get target file folder fail"))?;
    DirBuilder::new()
        .recursive(true)
        .create(target_file_folder)
        .await?;
    let mut target = File::create(&target_file_name).await?;
    let mut digest_calc = Hasher::new();
    let mut file_size_calc: u64 = 0;
    async fn process_result(
        result: Result<PartData>,
        target: &mut File,
        digest: &mut Hasher,
        file_size: &mut u64,
    ) -> Result<()> {
        let (skip, take, result) = result?;
        target.seek(SeekFrom::Start(skip)).await?;
        target.write_all(&result).await?;
        digest.update(&result);
        *file_size += take;
        Ok(())
    }
    if parts == 1 {
        let result = if from_local {
            fileutil::get_full_of_file(&local_source_file_name).await
        } else {
            get_full_of_file(base_url, catalog, &source_file_name).await
        };
        process_result(result, &mut target, &mut digest_calc, &mut file_size_calc).await?;
    } else {
        let mut results: Vec<task::JoinHandle<Result<PartData>>> =
            Vec::with_capacity(parts as usize);
        for i in 0..parts as usize {
            let skip = i as u64 * part_size;
            let take = part_size;
            let source_file_name = source_file_name.clone();
            let local_source_file_name = local_source_file_name.clone();
            let base_url: String = base_url.into();
            let catalog: String = catalog.into();
            results.push(task::spawn(async move {
                if from_local {
                    fileutil::get_part_of_file(&local_source_file_name, skip, take).await
                } else {
                    get_part_of_file(&base_url, &catalog, &source_file_name, skip, take).await
                }
            }));
        }
        for result in results {
            let result = result.await?;
            process_result(result, &mut target, &mut digest_calc, &mut file_size_calc).await?;
        }
    }

    let digest_calc = digest_calc.finalize();
    let digest_calc = format!("{}", digest_calc.to_hex());

    if file_size_calc != file_size {
        Err(anyhow!(
            "file size check error, expect: {}, got: {}",
            file_size,
            file_size_calc
        ))
    } else if digest_calc != digest {
        Err(anyhow!(
            "file hash check error, expect: {}, got: {}",
            digest,
            digest_calc
        ))
    } else {
        Ok((digest_calc, file_size_calc, parts, part_size, from_local))
    }
}

//(digest,size,name)
fn parse_file_list(str: &str) -> Vec<(&str, u64, &str)> {
    let file_list: Vec<(&str, u64, &str)> = str
        .split(EOL)
        .filter(|x| !x.is_empty())
        .map(|x| {
            let mut parts: Vec<&str> = x.split(',').collect();
            let file_name = parts.pop().unwrap();
            let size = parts.pop().unwrap();
            let size = u64::from_str_radix(&size, 10).unwrap();
            let digest = parts.pop().unwrap();
            (digest, size, file_name)
        })
        .collect();
    //file_list.sort_by_key(|x| x.0);
    file_list
}
pub async fn download_files(
    config: &Value,
    download_all: bool,
    max_tasks: u64,
    catalog: &str,
) -> Result<()> {
    use std::collections::{HashMap, HashSet};
    use std::ffi::OsStr;
    let client_config = &config["client"];
    let kill_running = client_config["kill_running_exe"].bool(true);
    let base_url = base_url(client_config);
    let catalog_config = &config[catalog];
    let part_size = catalog_config["part_size"].u64(1024 * 1024);
    let (_, _, bytes) = get_full_of_file(&base_url, catalog, "filelist.txt").await?;
    let remote_file_list_bytes = bytes.clone();
    let remote_file_list: String = String::from_utf8(bytes)?;
    let remote_file_list: Vec<(&str, u64, &str)> = parse_file_list(&remote_file_list);
    let file_count = remote_file_list.len();
    let file_size = remote_file_list.iter().map(|x| x.1).sum::<u64>();
    let path = client_config["path"].str("d:/tcsoftV6");
    let max_tasks = client_config["max_tasks"].u64(max_tasks);
    let local_file_list = fs::read_to_string(String::from(path) + "/filelist.txt")
        .await
        .unwrap_or("".to_string());
    let local_file_list: Vec<(&str, u64, &str)> = parse_file_list(&local_file_list);

    //(file_name,(digest,file_size))
    let local_file_list: HashMap<&str, (&str, u64)> = local_file_list
        .iter()
        .filter(|x| !x.2.is_empty())
        .map(|x| (x.2, (x.0, x.1)))
        .collect();

    //filter different files (digest,file_size,file_name)
    let remote_file_list: Vec<(&str, u64, &str)> = remote_file_list
        .into_iter()
        .filter(|(digest, file_size, file_name)| {
            if file_name.to_lowercase().ends_with("filer.json")
                || file_name.to_lowercase().ends_with("filer.exe.new")
            {
                false
            } else {
                if download_all {
                    true
                } else {
                    local_file_list
                        .get(file_name)
                        .map(|(local_digest, local_size)| {
                            !(local_digest == digest && local_size == file_size)
                        })
                        .unwrap_or(true)
                }
            }
        })
        .collect();
    let download_count = remote_file_list.len();
    let download_size = remote_file_list.iter().map(|x| x.1).sum::<u64>();

    if kill_running {
        let exe_list = remote_file_list
            .iter()
            .map(|x| Path::new(x.2))
            .filter(|x| x.extension().unwrap_or(OsStr::new("")) == "exe")
            .map(|x| x.file_name().unwrap().to_str().unwrap())
            .filter(|x| x.to_lowercase() != "filer.exe")
            .collect::<HashSet<&str>>();
        if !exe_list.is_empty() {
            print!("Kill running exe: ");
            for image_name in exe_list {
                print!("{}..", image_name);
                match kill_running_exe(image_name).await {
                    Ok(_) => (),
                    Err(e) => println!("失败: {:?}", e),
                };
            }
        }
        println!();
    }

    //(digest,(file_size,[file_name...]))
    let mut unique_digest_list: HashMap<String, (u64, Vec<String>, bool)> = HashMap::new();
    remote_file_list
        .iter()
        .for_each(|(digest, file_size, file_name)| {
            if let Some(val) = unique_digest_list.get_mut(*digest) {
                val.1.push(file_name.to_string());
                assert_eq!(val.0, *file_size);
            } else {
                unique_digest_list.insert(
                    digest.to_string(),
                    (*file_size, vec![file_name.to_string()], false),
                );
            }
        });

    use std::sync::{Arc, Mutex};
    let unique_digest_list = Arc::new(Mutex::new(unique_digest_list));

    //return (source_file_name,from_local)
    let get_source_file = |file_name: &str, digest: &str| -> (String, bool) {
        if let Ok(unique_digest_list) = unique_digest_list.lock() {
            if let Some((_, file_list, fetched)) = unique_digest_list.get(digest) {
                let first_file_name = file_list.get(0).unwrap().clone();
                (first_file_name, *fetched)
            } else {
                (file_name.to_string(), false)
            }
        } else {
            (file_name.to_string(), false)
        }
    };
    let set_source_file = |digest: &str| -> bool {
        if let Ok(mut unique_digest_list) = unique_digest_list.lock() {
            if let Some((_, _, fetched)) = unique_digest_list.get_mut(digest) {
                *fetched = true;
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    let mut download_error_count: usize = 0;
    let mut i: usize = 0;
    let mut print_count: usize = 0;

    println!("Download {} ...", catalog);
    while i < download_count {
        let mut task_count = 0u64;
        let mut results: Vec<(
            String,
            task::JoinHandle<Result<(String, u64, u64, u64, bool)>>,
        )> = Vec::with_capacity(max_tasks as usize);
        while task_count < max_tasks && i < download_count {
            let (digest, file_size, file_name) = *(remote_file_list
                .get(i)
                .ok_or(anyhow!("remote_file_list.get() error"))?);
            let (source_file_name, from_local) = get_source_file(file_name, digest);
            let base_url: String = base_url.clone();
            let catalog: String = catalog.into();
            let path: String = path.into();
            let digest: String = digest.into();
            let (task_add, part_size) = calc_parts(file_size, part_size, MAX_SPLIT_PARTS);
            let file_name_saved = file_name.into();
            let file_name: String = file_name.into();
            task_count += task_add;
            let result = task::spawn(async move {
                download_file(
                    &base_url,
                    &catalog,
                    &path,
                    &file_name,
                    file_size,
                    part_size,
                    &digest,
                    &source_file_name,
                    from_local,
                )
                .await
            });
            results.push((file_name_saved, result));
            i += 1;
        }
        for result in results {
            print_count += 1;
            let (file_name, result) = result;
            let result = result.await?;
            match result {
                Ok((digest, file_size, parts, _part_size, from_local)) => {
                    set_source_file(&digest);
                    println!(
                        ">>{: ^#4} {} {}={} pack{} ...{}",
                        print_count,
                        file_name,
                        file_size,
                        parts,
                        if parts > 1 { "s" } else { "" },
                        if from_local { "locally copied" } else { "" }
                    );
                }
                Err(e) => {
                    download_error_count += 1;
                    println!(">>{: ^#4} {} {:?}", print_count, file_name, e);
                }
            }
        }
    }

    if download_count - download_error_count > 0 {
        println!("Write filelist.txt which content from server");
        let file_name = String::from(path) + "/filelist.txt";
        let mut file = File::create(&file_name).await?;
        file.write_all(&remote_file_list_bytes).await?;
    }
    println!(
        "Total {} files with size {}, download {} files with size {} with failure count {}.",
        file_count,
        Byte::from_bytes(file_size as u128).get_appropriate_unit(false),
        download_count,
        Byte::from_bytes(download_size as u128).get_appropriate_unit(false),
        download_error_count
    );
    if download_count > 0 {
        println!(
            "Max concurrent {} tasks, each pack size {}",
            max_tasks,
            Byte::from_bytes(part_size as u128).get_appropriate_unit(true)
        );
    }
    Ok(())
}
