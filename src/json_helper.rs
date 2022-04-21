use serde_json::Value;
pub trait JsonHelper {
    //fn str(&self, default: &str) -> &str;
    fn str(&self, default: &'static str) -> &str;
    fn string(&self, default: &str) -> String;
    fn bool(&self, default: bool) -> bool;
    fn u64(&self, default: u64) -> u64;
    fn i64(&self, default: i64) -> i64;
    fn f64(&self, default: f64) -> f64;
    fn xml(&self) -> String;
}
impl JsonHelper for Value {
    fn str(&self, default: &'static str) -> &str {
        self.as_str().unwrap_or(default)
    }
    fn string(&self, default: &str) -> String {
        self.as_str()
            .map(|x| x.to_owned())
            .unwrap_or(default.to_owned())
    }
    fn bool(&self, default: bool) -> bool {
        self.as_bool().unwrap_or(default)
    }
    fn i64(&self, default: i64) -> i64 {
        self.as_i64().unwrap_or(default)
    }
    fn u64(&self, default: u64) -> u64 {
        self.as_u64().unwrap_or(default)
    }
    fn f64(&self, default: f64) -> f64 {
        self.as_f64().unwrap_or(default)
    }
    fn xml(&self) -> String {
        fn xml_process(json: &Value, last_key: &str) -> String {
            match json {
                Value::Object(obj) => {
                    let xml: Vec<String> = obj
                        .iter()
                        .map(|(k, v)| match v {
                            Value::Array(_) => xml_process(v, k),
                            _ => format!("<{}>{}</{}>", k, xml_process(v, k), k),
                        })
                        .collect();
                    xml.join("")
                }
                Value::String(v) => v.to_string(),
                Value::Bool(v) => (if *v { "true" } else { "false" }).to_string(),
                Value::Number(v) => v.as_f64().unwrap().to_string(),
                Value::Array(v) => {
                    let xml: Vec<String> = {
                        v.iter()
                            .map(
                                |x| format!("<{}>{}</{}>", last_key, xml_process(x, ""), last_key,),
                            )
                            .collect()
                    };
                    xml.join("")
                }
                Value::Null => "".into(),
            }
        }
        xml_process(self, "")
    }
}

#[cfg(test)]
mod tests {
    use super::JsonHelper;
    use serde_json::json;
    #[test]
    fn test_json_helper() {
        let js = json!({"a":"ebill.json","b":{"addr":"beijing","age":80}});
        assert_eq!(js["a"].str("missed"), "ebill.json");
        assert_eq!(js["b"]["age"].i64(0), 80);
        assert_eq!(js["b"]["addr"].str("missed"), "beijing");

        let js = json!({"a":["ebill.json","abc",100,123.45]});
        assert_eq!(js["a"][1].str("missed"), "abc");
        assert_eq!(js["a"][2].i64(-1), 100);
        assert_eq!(js["a"][3].f64(-1.0), 123.45);
        assert_eq!(js["a"][4].str("missed"), "missed");
    }
    #[test]
    fn test_json_to_xml() {
        let json = json!({"result":{
           "age":50,
           "name":"xiao",
           "list":[1,"hhh",3],
           "male":{"sex":true,"width":10.2}
        }});
        assert_eq!(json.xml(),"<result><age>50</age><list>1</list><list>hhh</list><list>3</list><male><sex>true</sex><width>10.2</width></male><name>xiao</name></result>".to_owned());
    }
}
