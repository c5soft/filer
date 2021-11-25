#![allow(dead_code)]
use anyhow::{anyhow, Result};

pub fn base16_encode(info: &str) -> Result<String> {
    let mut bytes: Vec<u8> = info.as_bytes().into();
    bytes.reverse();
    let bytes = bytes
        .iter()
        .map(|x| format!("{:2x}", !x))
        .collect::<Vec<String>>();
    let bytes = bytes.join("");
    Ok(bytes)
}

pub fn base16_decode(base16: &str) -> Result<String> {
    let str_len = base16.len();
    if str_len % 2 == 1 {
        return Err(anyhow!("base16_decode error: string length must be 2x"));
    }
    let bytes_count = str_len / 2;
    let mut bytes: Vec<u8> = Vec::with_capacity(bytes_count);
    for i in 0usize..bytes_count {
        let i = i * 2;
        let hex = base16.get(i..i + 2).ok_or(anyhow!("base16_decode error:get hex fail"))?;
        let byte = u8::from_str_radix(hex, 16)?;
        bytes.push(!byte);
    }
    bytes.reverse();
    String::from_utf8(bytes).map_err(|e| anyhow!("base16_decode/from_utf8 error {:?}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_base16_decode() {
        let base16 = "8268521a764e1984"; //"7be6b189e5ad977d" ;
        let result = base16_decode(base16).unwrap();
        assert_eq!(result, "{汉字}");
    }
    #[test]
    fn test_base16_encode() {
        let info = "{汉字}";
        let result = base16_encode(info).unwrap();
        assert_eq!(result, "8268521a764e1984");
    }
}
