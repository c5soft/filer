#![allow(dead_code)]

use anyhow::{anyhow, Result};
use byte_unit::Byte;
use std::io::SeekFrom;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::task;

pub type PartData = (u64, u64, Vec<u8>);
pub const EOL: &str = "\r\n";
pub const MAX_SPLIT_PARTS: u64 = 128;

pub async fn get_file_size(file_name: &str) -> Result<u64> {
    let meta = fs::metadata(file_name).await?;
    Ok(meta.len())
}

pub async fn get_full_of_file(file_name: &str) -> Result<PartData> {
    let mut file = File::open(file_name).await?;
    let mut result: Vec<u8> = Vec::new();
    let size = file.read_to_end(&mut result).await?;
    Ok((0, size as u64, result))
}

pub async fn get_part_of_file(file_name: &str, skip: u64, take: u64) -> Result<PartData> {
    let mut file = File::open(file_name).await?;
    if skip > 0 {
        file.seek(SeekFrom::Start(skip)).await?;
    }
    let mut result = vec![0u8; take as usize];
    let size = file.read_exact(&mut result).await;
    match size {
        Ok(size) => Ok((skip, size as u64, result)),
        Err(err) => {
            if format!("{:?}", err) == r#"Custom { kind: UnexpectedEof, error: "early eof" }"# {
                file.seek(SeekFrom::Start(skip)).await.unwrap();
                let mut result: Vec<u8> = Vec::new();
                let size = file.read_to_end(&mut result).await?;
                Ok((skip, size as u64, result))
            } else {
                Err(anyhow!(err))
            }
        }
    }
}

pub fn get_dir_file_names(path: &str) -> Result<Vec<String>> {
    let mut results: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(&path)?;
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        let file_name = entry.file_name();
        let file_name: String = file_name.to_str().unwrap().into();
        let full_name = String::from(path) + "/" + &file_name;
        if meta.is_dir() {
            let mut files = get_dir_file_names(&full_name)?;
            results.append(&mut files);
        } else {
            results.push(full_name);
        }
    }
    Ok(results)
}

#[cfg(feature = "digest")]
//return [(file_name,size,digest)...]
pub async fn get_dir_file_size_and_digest(
    path: &str,
    part_size: u64,
    max_tasks: u64,
    show_progress: bool,
) -> Result<Vec<(String, u64, String)>> {
    let path = String::from(path);
    let files = task::spawn_blocking(move || get_dir_file_names(&path)).await??;
    let file_count = files.len();
    let mut results: Vec<(String, u64, String)> = Vec::with_capacity(file_count);
    let mut calc_error_count: usize = 0;
    let mut print_count: u64 = 0;
    let mut i: usize = 0;

    while i < file_count {
        let mut task_count = 0u64;
        let mut tasks: Vec<task::JoinHandle<Result<(String, u64, String)>>> =
            Vec::with_capacity(file_count);
        while task_count < max_tasks && i < file_count {
            let file = files.get(i).ok_or(anyhow!("digest files.get() error"))?;
            let file_name = file.clone();
            let task = task::spawn(async move {
                get_file_size_and_digest(&file_name, part_size, max_tasks)
                    .await
                    .map(|(size, digest)| (file_name, size, digest))
            });
            tasks.push(task);
            task_count += 1;
            i += 1;
        }
        for task in tasks {
            let result = task.await?;
            print_count += 1;
            match result {
                Ok((file_name, file_size, digest)) => {
                    if show_progress {
                        println!(
                            ">>{: ^#4} {} {} ...",
                            print_count,
                            &file_name,
                            Byte::from_bytes(file_size as u128).get_appropriate_unit(false)
                        );
                    }
                    results.push((file_name, file_size, digest));
                }
                Err(e) => {
                    calc_error_count += 1;
                    if show_progress {
                        println!(">>{: ^#4} {:?}", print_count, e);
                    }
                }
            }
        }
    }
    if show_progress && calc_error_count > 0 {
        println!("Total digest calc error count {}", calc_error_count);
    }
    Ok(results)
}

pub fn calc_parts(file_size: u64, part_size: u64, max_split_parts: u64) -> (u64, u64) {
    let parts = (file_size + part_size - 1) / part_size;
    if parts <= max_split_parts {
        (parts, part_size)
    } else {
        calc_parts(file_size, part_size * 2, max_split_parts)
    }
}

//return [(file_name,size)...]
pub async fn get_dir_file_size(path: &str) -> Result<Vec<(String, u64)>> {
    let path = String::from(path);
    let files = task::spawn_blocking(move || get_dir_file_names(&path)).await??;
    let file_count = files.len();
    let mut results: Vec<(String, u64)> = Vec::with_capacity(file_count);
    let mut tasks: Vec<task::JoinHandle<Result<(String, u64)>>> = Vec::with_capacity(file_count);
    for file in files {
        let file_name = file;
        let task = task::spawn(async move {
            get_file_size(&file_name)
                .await
                .map(|size| (file_name, size))
        });
        tasks.push(task);
    }
    for task in tasks {
        let result = task.await??;
        results.push(result);
    }
    Ok(results)
}

#[cfg(feature = "digest")]
pub async fn get_file_size_and_digest(
    file_name: &str,
    part_size: u64,
    max_tasks: u64,
) -> Result<(u64, String)> {
    use blake3::Hasher;
    let file_size = get_file_size(file_name).await?;
    let (parts, part_size) = calc_parts(file_size, part_size, max_tasks / 2);
    // println!(
    //     "Calc MD5 for {} with size {},splited {} parts*{} ...",
    //     file_name, file_size, parts, part_size
    // );

    let mut digest = Hasher::new();
    let mut results: Vec<task::JoinHandle<Result<PartData>>> = Vec::with_capacity(parts as usize);
    if parts == 1 {
        let (_, _, part) = get_full_of_file(file_name).await?;
        digest.update(&part);
    } else {
        for i in 0..parts as usize {
            let skip = i as u64 * part_size;
            let take = part_size;
            let file_name: String = file_name.into();
            let result = task::spawn(async move { get_part_of_file(&file_name, skip, take).await });
            results.push(result);
        }
        for result in results {
            let result = result.await?;
            let (_, _, part) = result?;
            digest.update(&part);
        }
    }
    let digest = digest.finalize();
    let digest = format!("{}", digest.to_hex());
    Ok((file_size, digest))
}

// async fn get_file_mpsc(
//     source_file_name: &str,
//     source_file_size: u64,
//     part_size: u64,
//     target_file_name: &str,
// ) -> Result<u64> {
//     use tokio::sync::mpsc;
//     let parts = (source_file_size + part_size - 1) / part_size;
//     let source_file_name = String::from(source_file_name);
//     println!(
//         "get {} save as {} with size {},splited {} parts*{} ...",
//         source_file_name, target_file_name, source_file_size, parts, part_size
//     );
//     let (tx, mut rx) = mpsc::channel::<Result<PartData>>(1000);
//     if parts == 1 {
//         let result = get_full_of_file(&source_file_name).await;
//         tx.send(result).await.unwrap();
//     } else {
//         for i in 0..parts as usize {
//             let tx = tx.clone();
//             let skip = i as u64 * part_size;
//             let take = part_size;
//             let source_file_name = source_file_name.clone();
//             task::spawn(async move {
//                 let result = get_part_of_file(&source_file_name, skip, take).await;
//                 tx.send(result).await.unwrap();
//             });
//         }
//     }
//     drop(tx);

//     let mut target = File::create(target_file_name).await?;

//     while let Some(result) = rx.recv().await {
//         let (skip, _, result) = result?;
//         // println!("get:<{}>", String::from_utf8(result.clone()).unwrap());
//         target.seek(SeekFrom::Start(skip)).await?;
//         target.write_all(&result).await?;
//     }

//     let target_file_size = get_file_size(&target_file_name).await?;

//     assert_eq!(target_file_size, source_file_size);

//     Ok(target_file_size)
// }

pub async fn write_string_to_file(str: &str, file_name: &str) -> Result<bool> {
    let mut target = File::create(file_name).await?;
    let bytes: Vec<u8> = str.as_bytes().into();
    target.write_all(&bytes).await?;
    Ok(true)
}
pub async fn get_file(
    source_file_name: &str,
    source_file_size: u64,
    part_size: u64,
    target_file_name: &str,
) -> Result<u64> {
    let parts = (source_file_size + part_size - 1) / part_size;
    let source_file_name = String::from(source_file_name);
    println!(
        "get {} save as {} with size {},splited {} parts*{} ...",
        source_file_name, target_file_name, source_file_size, parts, part_size
    );
    let mut target = File::create(target_file_name).await?;
    async fn process_result(result: Result<PartData>, target: &mut File) -> Result<()> {
        let (skip, _, result) = result?;
        target.seek(SeekFrom::Start(skip)).await?;
        target.write_all(&result).await?;
        Ok(())
    }
    if parts == 1 {
        let result = get_full_of_file(&source_file_name).await;
        process_result(result, &mut target).await?;
    } else {
        let mut results: Vec<task::JoinHandle<Result<PartData>>> =
            Vec::with_capacity(parts as usize);
        for i in 0..parts as usize {
            let skip = i as u64 * part_size;
            let take = part_size;
            let source_file_name = source_file_name.clone();
            results.push(task::spawn(async move {
                get_part_of_file(&source_file_name, skip, take).await
                //tx.send(result).await.unwrap();
            }));
        }
        for result in results {
            let result = result.await?;
            process_result(result, &mut target).await?;
        }
    }

    let target_file_size = get_file_size(&target_file_name).await?;

    assert_eq!(target_file_size, source_file_size);

    Ok(target_file_size)
}

#[cfg(feature = "digest")]
pub async fn get_file_and_digest(
    source_file_name: &str,
    source_file_size: u64,
    part_size: u64,
    target_file_name: &str,
) -> Result<(u64, String)> {
    use blake3::Hasher;
    let parts = (source_file_size + part_size - 1) / part_size;
    let source_file_name = String::from(source_file_name);
    println!(
        "get {} save as {} with size {},splited {} parts*{} ...",
        source_file_name, target_file_name, source_file_size, parts, part_size
    );
    let mut target = File::create(target_file_name).await?;
    let mut digest = Hasher::new();
    async fn process_result(
        result: PartData,
        target: &mut File,
        digest: &mut Hasher,
    ) -> Result<()> {
        let (skip, _, result) = result;
        target.seek(SeekFrom::Start(skip)).await?;
        target.write_all(&result).await?;
        digest.update(&result);
        Ok(())
    }
    if parts == 1 {
        let result = get_full_of_file(&source_file_name).await?;
        process_result(result, &mut target, &mut digest).await?;
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
            let result = result.await??;
            process_result(result, &mut target, &mut digest).await?;
        }
    }

    let digest = digest.finalize();
    let digest = format!("{}", digest.to_hex());

    let target_file_size = get_file_size(&target_file_name).await?;

    assert_eq!(target_file_size, source_file_size);

    Ok((target_file_size, digest))
}

#[cfg(feature = "digest")]
pub async fn refresh_dir_files_digest(
    path: &str,
    list_file_name: &str,
    part_size: u64,
    max_tasks: u64,
    show_repeat: bool,
) -> Result<()> {
    use std::collections::HashMap;
    let path_len = path.len();
    println!("Calc digest for files in {}...", path);
    //(file_name,file_size,digest)
    let results = get_dir_file_size_and_digest(path, part_size, max_tasks, true).await?;
    let list_file_name = String::from(path.to_lowercase()) + "/" + list_file_name;
    let file_list_iter = results
        .iter()
        .filter(|x| x.0.to_lowercase() != list_file_name);
    let total_size = file_list_iter
        .clone()
        .fold(0, |sum, (_, file_size, _)| sum + file_size);
    let total_size_with_unit = Byte::from_bytes(total_size as u128).get_appropriate_unit(false);
    let file_list_joined = file_list_iter
        .clone()
        .map(|x| {
            let file_name = x.0.get(path_len + 1..).unwrap();
            format!("{},{},{}", x.2, x.1, file_name)
        })
        .fold("".to_string(), |joined, x| {
            let sep = if joined.is_empty() { "" } else { EOL };
            joined + sep + &x
        });
    write_string_to_file(&file_list_joined, &list_file_name).await?;
    println!(
        "\nTotal {} files with size {},digest checksum write to {}",
        results.len(),
        total_size_with_unit,
        &list_file_name
    );
    if show_repeat {
        let mut unique_digest_list: HashMap<String, (u64, Vec<String>)> = HashMap::new();
        file_list_iter.for_each(|(file_name, file_size, digest)| {
            if let Some(val) = unique_digest_list.get_mut(digest) {
                val.1.push(file_name.clone());
                assert_eq!(val.0, *file_size);
            } else {
                unique_digest_list.insert(digest.clone(), (*file_size, vec![file_name.clone()]));
            }
        });
        let unique_digest_iter = unique_digest_list.iter();
        let unique_digest_size = unique_digest_iter
            .clone()
            .fold(0, |sum, (_, (size, _))| sum + size);
        let repeat_digest_iter = unique_digest_iter
            .clone()
            .filter(|(_, (_, names))| names.len() > 1);
        let repeat_file_sizesum_and_filecount_and_groupcount = repeat_digest_iter
            .clone()
            .map(|(_, (file_size, names))| (*file_size, (names.len() - 1) as u64))
            .fold(
                (0u64, 0u64, 0u64),
                |(repeat_size_sum, repeat_file_count, repeat_group_count),
                 (file_size, repeat_count)| {
                    (
                        repeat_size_sum + file_size * repeat_count,
                        repeat_file_count + repeat_count,
                        repeat_group_count + 1,
                    )
                },
            );
        repeat_digest_iter
            .enumerate()
            .for_each(|(i, (digest, (file_size, repeat_name_list)))| {
                let file_size_with_unit =
                    Byte::from_bytes(*file_size as u128).get_appropriate_unit(false);
                println!(
                    "Repeat file group #{} file size: {} digest:{}",
                    i + 1,
                    file_size_with_unit,
                    digest
                );
                repeat_name_list.iter().enumerate().for_each(|(i, x)| {
                    println!(">>{: ^3} {}", i + 1, x);
                });
            });
        let repeat_file_sizesum_calc = total_size - unique_digest_size;
        let (repeat_file_sizesum, repeat_file_count, repeat_group_count) =
            repeat_file_sizesum_and_filecount_and_groupcount;
        assert_eq!(repeat_file_sizesum, repeat_file_sizesum_calc);
        println!(
            "\nInclude {} group repeat files with count {} with size {}",
            repeat_group_count,
            repeat_file_count,
            Byte::from_bytes(repeat_file_sizesum as u128).get_appropriate_unit(false),
        );
    };
    Ok(())
}

pub async fn kill_running_exe(image_name: &str) -> Result<(i32, String)> {
    use std::process::Stdio;
    use tokio::process::Command;
    let mut child = Command::new("taskkill")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg("/IM")
        .arg(image_name)
        .arg("/T")
        .arg("/F")
        .spawn()?;

    // Await until the command completes
    // use std::process::Output;
    // let Output {
    //     status,
    //     stdout,
    //     stderr,
    // } = child.wait_with_output().await?;
    // let out = String::from_utf8(stdout)?;
    // let err = String::from_utf8(stderr)?;
    // println!("stdout:{},stderr:{}", out, err);
    let status = child.wait().await?;
    let exit_code = status
        .code()
        .ok_or(anyhow!("kill_running_exe get exit_code fail"))?;
    Ok((exit_code, String::from(image_name)))
}
