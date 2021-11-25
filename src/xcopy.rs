use crate::fileutil::{
    calc_parts, get_dir_file_size, get_file_size, get_full_of_file, get_part_of_file,
    kill_running_exe, PartData, MAX_SPLIT_PARTS,
};
use crate::json_helper::JsonHelper;
use anyhow::{anyhow, Result};
use byte_unit::Byte;
use serde_json::Value;
use std::io::SeekFrom;
use std::path::Path;
use tokio::fs::{self, DirBuilder, File};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::task;

async fn xcopy_file(
    source_path: &str,
    target_path: &str,
    file_name: &str,
    file_size: u64,
    part_size: u64,
) -> Result<(String, u64, u64, u64)> {
    let source_file_name = source_path.to_string() + "/" + file_name;
    let target_file_name = if file_name.to_lowercase().ends_with("filer.exe") {
        String::from(target_path) + "/" + file_name + ".new"
    } else {
        String::from(target_path) + "/" + file_name
    };
    //println!("copy {} to {}", source_file_name, target_file_name);
    let (parts, part_size) = calc_parts(file_size, part_size, MAX_SPLIT_PARTS);
    let target_file_folder = Path::new(&target_file_name)
        .parent()
        .ok_or(anyhow!("get target file folder fail"))?;
    DirBuilder::new()
        .recursive(true)
        .create(target_file_folder)
        .await?;
    let mut target = File::create(&target_file_name).await?;
    let mut file_size_calc: u64 = 0;
    async fn process_result(
        result: Result<PartData>,
        target: &mut File,
        file_size: &mut u64,
    ) -> Result<()> {
        let (skip, take, result) = result?;
        target.seek(SeekFrom::Start(skip)).await?;
        target.write_all(&result).await?;
        *file_size += take;
        Ok(())
    }
    if parts == 1 {
        let result = get_full_of_file(&source_file_name).await;
        process_result(result, &mut target, &mut file_size_calc).await?;
    } else {
        let mut results: Vec<task::JoinHandle<Result<PartData>>> =
            Vec::with_capacity(parts as usize);
        for i in 0..parts as usize {
            let skip = i as u64 * part_size;
            let take = part_size;
            let source_file_name = source_file_name.clone();
            results.push(task::spawn(async move {
                get_part_of_file(&source_file_name, skip, take).await
            }));
        }
        for result in results {
            let result = result.await?;
            process_result(result, &mut target, &mut file_size_calc).await?;
        }
    }

    if file_size_calc != file_size {
        Err(anyhow!(
            "{} file size check error, expect: {}, got: {}",
            file_name,
            file_size,
            file_size_calc
        ))
    } else {
        Ok((String::from(file_name), file_size_calc, parts, part_size))
    }
}

pub async fn xcopy_files(config: &Value, source_path: &str, target_path: &str,max_tasks:u64) -> Result<()> {
    fn fine_path(path: &str) -> Result<String> {
        let path = path.to_string().replace("\\", "/");
        let path: String = if path.ends_with("/") {
            path.get(0..path.len() - 1)
                .ok_or(anyhow!("fine_path:strip / of path fail"))?
                .to_string()
        } else {
            path
        };
        Ok(path)
    }
    use std::collections::HashSet;
    use std::ffi::OsStr;
    let original_source_path = fine_path(source_path)?;
    let source_path = original_source_path.as_str();
    let target_path = fine_path(target_path)?;
    let target_path = target_path.as_str();
    let client_config = &config["xcopy"];
    let kill_running = client_config["kill_running_exe"].bool(false);
    let part_size = client_config["part_size"].u64(1024 * 1024);
    let max_tasks = client_config["max_tasks"].u64(max_tasks);
    let meta = fs::metadata(source_path).await?;
    let (source_file_list, source_path, source_path_is_file) = if meta.is_dir() {
        (
            get_dir_file_size(source_path).await?,
            source_path.to_string(),
            false,
        )
    } else {
        let file_name = source_path.to_string();
        let file_size = get_file_size(source_path).await?;
        let source_path = Path::new(source_path)
            .parent()
            .ok_or(anyhow!("get parent of source_path fail"))?;
        let source_path = source_path
            .to_str()
            .ok_or(anyhow!("parent of source_path to_str fail"))?;
        let source_path = fine_path(source_path)?;
        (vec![(file_name, file_size)], source_path, true)
    };
    let source_path_len = source_path.len();
    let source_file_list = source_file_list
        .into_iter()
        .map(|x| {
            let file_name = x.0.get(source_path_len + 1..).unwrap().to_string();
            (file_name, x.1)
        })
        .collect::<Vec<(String, u64)>>();
    let file_count = source_file_list.len();
    let file_size = source_file_list.iter().map(|x| x.1).sum::<u64>();

    if kill_running {
        let exe_list = source_file_list
            .iter()
            .map(|x| Path::new(&x.0))
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

    let mut xcopy_error_count: usize = 0;
    let mut i: usize = 0;
    let mut print_count: usize = 0;

    if source_path_is_file {
        println!("Copy {} to {}/ ...", original_source_path, target_path);
    } else {
        println!("XCopy files from {}/ to {}/ ...", source_path, target_path);
    }
    while i < file_count {
        let mut task_count = 0u64;
        let mut results: Vec<task::JoinHandle<Result<(String, u64, u64, u64)>>> =
            Vec::with_capacity(max_tasks as usize);
        while task_count < max_tasks && i < file_count {
            let (file_name, file_size) = source_file_list
                .get(i)
                .ok_or(anyhow!("xcopy source_file_list.get() error"))?;
            let file_size = *file_size;
            let (task_add, part_size) = calc_parts(file_size, part_size, MAX_SPLIT_PARTS);
            let file_name: String = file_name.into();
            let source_path = source_path.to_string();
            let target_path = target_path.to_string();
            task_count += task_add;
            results.push(task::spawn(async move {
                xcopy_file(&source_path, &target_path, &file_name, file_size, part_size).await
            }));
            i += 1;
        }
        for result in results {
            print_count += 1;
            let result = result.await?;
            match result {
                Ok((file_name, file_size, parts, _part_size)) => {
                    println!(
                        ">>{: ^#4} {} {}={} pack{} ...",
                        print_count,
                        file_name,
                        file_size,
                        parts,
                        if parts > 1 { "s" } else { "" }
                    );
                }
                Err(e) => {
                    xcopy_error_count += 1;
                    println!(">>{: ^#4} {:?}", print_count, e);
                }
            }
        }
    }

    println!(
        "Copy {} files with size {} from {}/ to {}/, with failure count {}.",
        file_count,
        Byte::from_bytes(file_size as u128).get_appropriate_unit(false),
        source_path,
        target_path,
        xcopy_error_count
    );
    println!(
        "Max concurrent {} tasks, each pack size {}",
        max_tasks,
        Byte::from_bytes(part_size as u128).get_appropriate_unit(true)
    );
    Ok(())
}
