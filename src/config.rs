use serde_json::value::Value;
use std::convert::AsRef;
use std::path::{Path, PathBuf};

pub fn get_config_file() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.set_extension("json");
    if !path.exists() {
        path = PathBuf::from(path.file_name().unwrap());
    }
    #[cfg(test)]
    if !path.exists() {
        path = PathBuf::from("filer.json");
    }
    if !path.exists() {
        panic!("Application config file: {:?} does not exist!", path);
    };
    path
}
fn read_json<P: AsRef<Path>>(path: P) -> Value {
    if let Ok(file) = std::fs::File::open(path) {
        let reader = std::io::BufReader::new(file);
        if let Ok(value) = serde_json::from_reader(reader) {
            value
        } else {
            Value::Null
        }
    } else {
        Value::Null
    }
}

pub fn from(path: PathBuf) -> Value {
    read_json(path)
}

#[cfg(test)]
pub fn new() -> Value {
    let path = get_config_file();
    from(path)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn test_config_new() {
        let config = super::new();
        assert_eq!(
            config["server"],
            json!({
                "static_path": "D:/Js/OnlyOne/public",
                "server_name": "OnlyOne",
                "http_active": true,
                "http_port": 80,
                "https_active": true,
                "https_port": 443,
                "https_cert": "cert.pem",
                "https_key": "key.pem"
            })
        );
        assert_eq!(config["config"]["https_port"], json!(443));
    }
}
