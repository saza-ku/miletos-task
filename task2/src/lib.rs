use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Cursor, Read},
};

use anyhow::{Error, Result};

struct SysctlConfigSchema {
    key: String,
    typ: SysctlConfigType,
}

impl SysctlConfigSchema {
    fn new(key: String, typ: String) -> Result<Self> {
        let typ = SysctlConfigType::from_string(&typ).unwrap();
        Ok(Self { key, typ })
    }
}

pub struct SysctlConfigLoader {
    schema: Vec<SysctlConfigSchema>,
}

#[derive(Clone)]
pub enum SysctlConfigValue {
    String(String),
    SysctlConfig(SysctlConfig),
}

#[derive(Clone)]
pub enum SysctlConfigType {
    Int,
    Float,
    String,
    Bool,
}

impl SysctlConfigType {
    fn from_string(s: &str) -> Result<Self> {
        match s {
            "int" => Ok(SysctlConfigType::Int),
            "float" => Ok(SysctlConfigType::Float),
            "string" => Ok(SysctlConfigType::String),
            "bool" => Ok(SysctlConfigType::Bool),
            _ => Err(Error::msg(format!("unknown type: {}", s))),
        }
    }
}

type SysctlConfig = HashMap<String, SysctlConfigValue>;

impl SysctlConfigLoader {
    pub fn new(path: &str) -> Self {
        let file = std::fs::read_to_string(path).unwrap();
        let r = BufReader::new(Cursor::new(file));
        let r = BufReader::new(r);
        let schema = Self::load_sysctl_schema_from_reader(r).unwrap();
        Self { schema }
    }

    fn load_sysctl_schema_from_reader<T: Read>(
        reader: BufReader<T>,
    ) -> Result<Vec<SysctlConfigSchema>> {
        let mut schema = vec![];
        for line in reader.lines() {
            let line = line?;
            Self::insert_schema_of_line(&mut schema, line)?;
        }
        Ok(schema)
    }

    fn insert_schema_of_line(schema: &mut Vec<SysctlConfigSchema>, line: String) -> Result<()> {
        if line.is_empty() {
            return Ok(());
        }

        let parts: Vec<&str> = line.splitn(2, "->").collect();
        if parts.len() != 2 {
            return Err(Error::msg("invalid line"));
        }

        let key = parts[0].trim();
        let value = parts[1].trim();

        let schema_elem = SysctlConfigSchema::new(key.to_string(), value.to_string())?;
        schema.push(schema_elem);

        Ok(())
    }

    pub fn load_sysctl(self: &Self, path: &str) -> Result<SysctlConfig> {
        let file = std::fs::read_to_string(path)?;
        let r = BufReader::new(Cursor::new(file));
        let r = BufReader::new(r);
        let result = self.load_sysctl_from_reader(r)?;
        self.validate(&result)?;
        Ok(result)
    }

    fn load_sysctl_from_reader<T: Read>(self: &Self, reader: BufReader<T>) -> Result<SysctlConfig> {
        let mut map = SysctlConfig::new();
        for line in reader.lines() {
            let line = line?;
            self.insert_entry_of_line(&mut map, line.as_str())?;
        }
        Ok(map)
    }

    fn insert_entry_of_line(self: &Self, map: &mut SysctlConfig, line: &str) -> Result<()> {
        let mut line = line;
        if line.is_empty() {
            return Ok(());
        }

        if line.starts_with("#") || line.starts_with(";") {
            return Ok(());
        }

        let error_or_ignore = if line.starts_with("-") {
            line = &line[1..];
            |_| Ok(())
        } else {
            |s| Err(Error::msg(s))
        };

        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            return error_or_ignore("invalid line");
        }
        let key = parts[0].trim();
        let value = parts[1].trim();
        if key.is_empty() || value.is_empty() || key.contains(' ') {
            return error_or_ignore("invalid line");
        }

        let keys = key.split('.').collect::<Vec<&str>>();

        let mut m = map;
        for i in 0..keys.len() {
            let key = keys[i];
            if i == keys.len() - 1 {
                m.insert(
                    key.to_string(),
                    SysctlConfigValue::String(value.to_string()),
                );
            } else {
                let next_m = m
                    .entry(key.to_string())
                    .or_insert_with(|| SysctlConfigValue::SysctlConfig(SysctlConfig::new()));
                if let SysctlConfigValue::SysctlConfig(next_m) = next_m {
                    m = next_m;
                } else {
                    return error_or_ignore("invalid line");
                }
            }
        }

        Ok(())
    }

    fn validate(self: &Self, m: &SysctlConfig) -> Result<()> {
        let mut keys = Self::get_all_keys(m);
        for schema in self.schema.iter() {
            keys.remove(&schema.key);
        }
        if !keys.is_empty() {
            return Err(Error::msg(format!("surplus keys: {:?}", keys)));
        }

        for schema in self.schema.iter() {
            let mut map = m;

            let key = &schema.key;
            let typ = &schema.typ;

            let keys = key.split('.').collect::<Vec<&str>>();
            let value = {
                let mut v = Err(Error::msg(format!("key not found: {}", key)));
                for i in 0..keys.len() {
                    let key = keys[i];
                    if i == keys.len() - 1 {
                        v = Ok(map.get(key));
                        break;
                    } else {
                        let next_m = map.get(key);
                        if next_m.is_none() {
                            v = Err(Error::msg(format!("not found: key={}", key)));
                            break;
                        }
                        if let SysctlConfigValue::SysctlConfig(next_m) = next_m.unwrap() {
                            map = next_m;
                        } else {
                            v = Err(Error::msg(format!("invalid value: key={}", key)));
                            break;
                        }
                    }
                }
                v
            }?;

            if value.is_none() {
                return Err(Error::msg(format!("key not found: key={}", key)));
            }

            let value = value.unwrap();
            match typ {
                SysctlConfigType::Int => {
                    if let SysctlConfigValue::String(v) = value {
                        if v.parse::<i64>().is_err() {
                            return Err(Error::msg(format!(
                                "invalid value: key={}, value={}, type={}",
                                key, v, "int"
                            )));
                        }
                    } else {
                        return Err(Error::msg(format!(
                            "invalid value: key={}, type={}",
                            key, "int"
                        )));
                    }
                }
                SysctlConfigType::Float => {
                    if let SysctlConfigValue::String(v) = value {
                        if v.parse::<f64>().is_err() {
                            return Err(Error::msg(format!(
                                "invalid value: key={}, value={}, type={}",
                                key, v, "float"
                            )));
                        }
                    } else {
                        return Err(Error::msg(format!(
                            "invalid value: key={}, type={}",
                            key, "float"
                        )));
                    }
                }
                SysctlConfigType::String => {}
                SysctlConfigType::Bool => {
                    if let SysctlConfigValue::String(v) = value {
                        if v != "true" && v != "false" {
                            return Err(Error::msg(format!(
                                "invalid value: key={}, value={}, type={}",
                                key, v, "bool"
                            )));
                        }
                    } else {
                        return Err(Error::msg(format!(
                            "invalid value: key={}, type={}",
                            key, "bool"
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    fn get_all_keys(m: &SysctlConfig) -> HashSet<String> {
        let mut keys = HashSet::new();
        Self::insert_key(m, "", &mut keys);
        keys
    }

    fn insert_key(m: &SysctlConfig, prev_key: &str, set: &mut HashSet<String>) -> () {
        for (k, v) in m.iter() {
            let key = if prev_key == "" {
                k.to_string()
            } else {
                format!("{}.{}", prev_key, k)
            };
            if let SysctlConfigValue::SysctlConfig(v) = v {
                Self::insert_key(v, &key, set);
            } else {
                set.insert(key.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn ok_str() {
        let test_data_value = "hoge = fuga
piyo = hoge
";

        let test_data_schema = "hoge -> string
piyo -> string
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let map = loader
            .load_sysctl(value_file.path().to_str().unwrap())
            .unwrap();

        let hoge = map.get("hoge").unwrap();
        if let SysctlConfigValue::String(v) = hoge {
            assert_eq!(v, "fuga");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "hoge");
        }

        let piyo = map.get("piyo").unwrap();
        if let SysctlConfigValue::String(v) = piyo {
            assert_eq!(v, "hoge");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "piyo");
        }
    }

    #[test]
    fn ok_nested_str() {
        let test_data_value = "hoge.fuga = hoge
hoge.piyo = hoge
";

        let test_data_schema = "hoge.fuga -> string
hoge.piyo -> string
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let map = loader
            .load_sysctl(value_file.path().to_str().unwrap())
            .unwrap();

        let hoge = map.get("hoge").unwrap();
        if let SysctlConfigValue::SysctlConfig(hoge) = hoge {
            let fuga = hoge.get("fuga").unwrap();
            if let SysctlConfigValue::String(v) = fuga {
                assert_eq!(v, "hoge");
            } else {
                panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "fuga");
            }

            let piyo = hoge.get("piyo").unwrap();
            if let SysctlConfigValue::String(v) = piyo {
                assert_eq!(v, "hoge");
            } else {
                panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "piyo");
            }
        } else {
            panic!("expected SysctlConfigValue::SysctlConfig, but got SysctlConfigValue::String: key={}", "hoge");
        }
    }

    #[test]
    fn ok_int() {
        let test_data_value = "hoge = 1
piyo = 2
";

        let test_data_schema = "hoge -> int
piyo -> int
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let map = loader
            .load_sysctl(value_file.path().to_str().unwrap())
            .unwrap();

        let hoge = map.get("hoge").unwrap();
        if let SysctlConfigValue::String(v) = hoge {
            assert_eq!(v, "1");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "hoge");
        }

        let piyo = map.get("piyo").unwrap();
        if let SysctlConfigValue::String(v) = piyo {
            assert_eq!(v, "2");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "piyo");
        }
    }

    #[test]
    fn ok_float() {
        let test_data_value = "hoge = 1.1
piyo = 2
";

        let test_data_schema = "hoge -> float
piyo -> float
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let map = loader
            .load_sysctl(value_file.path().to_str().unwrap())
            .unwrap();

        let hoge = map.get("hoge").unwrap();
        if let SysctlConfigValue::String(v) = hoge {
            assert_eq!(v, "1.1");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "hoge");
        }

        let piyo = map.get("piyo").unwrap();
        if let SysctlConfigValue::String(v) = piyo {
            assert_eq!(v, "2");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "piyo");
        }
    }

    #[test]
    fn ok_bool() {
        let test_data_value = "hoge = true
piyo = false
";

        let test_data_schema = "hoge -> bool
piyo -> bool
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let map = loader
            .load_sysctl(value_file.path().to_str().unwrap())
            .unwrap();

        let hoge = map.get("hoge").unwrap();
        if let SysctlConfigValue::String(v) = hoge {
            assert_eq!(v, "true");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "hoge");
        }

        let piyo = map.get("piyo").unwrap();
        if let SysctlConfigValue::String(v) = piyo {
            assert_eq!(v, "false");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "piyo");
        }
    }

    #[test]
    fn ng_int() {
        let test_data_value = "hoge = fuga
piyo = hoge
";

        let test_data_schema = "hoge -> int
piyo -> string
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let result = loader.load_sysctl(value_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn ng_float() {
        let test_data_value = "hoge = true
piyo = hoge
";

        let test_data_schema = "hoge -> float
piyo -> string
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let result = loader.load_sysctl(value_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn ng_bool() {
        let test_data_value = "hoge = 1
piyo = hoge
";

        let test_data_schema = "hoge -> bool
piyo -> string
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let result = loader.load_sysctl(value_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn ng_missing() {
        let test_data_value = "hoge = 1
piyo = hoge
";

        let test_data_schema = "hoge -> int
piyo -> string
fuga -> string
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let result = loader.load_sysctl(value_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn ng_missing_nested() {
        let test_data_value = "hoge.fuga = 1
piyo = false
";

        let test_data_schema = "hoge.fuga -> float
hoge.piyo -> bool
piyo -> string
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let result = loader.load_sysctl(value_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn ng_surplus() {
        let test_data_value = "hoge = 1
piyo = false
";

        let test_data_schema = "hoge -> int
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let result = loader.load_sysctl(value_file.path().to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn ng_surplus_nested() {
        let test_data_value = "hoge.fuga = 1
hoge.piyo = false
piyo = 1.1
fuga.fuga = fuga
";

        let test_data_schema = "hoge.fuga -> float
hoge.piyo -> bool
piyo -> float
";

        let mut value_file = NamedTempFile::new().unwrap();
        value_file.write_all(test_data_value.as_bytes()).unwrap();
        let mut schema_file = NamedTempFile::new().unwrap();
        schema_file.write_all(test_data_schema.as_bytes()).unwrap();

        let loader = SysctlConfigLoader::new(schema_file.path().to_str().unwrap());

        let result = loader.load_sysctl(value_file.path().to_str().unwrap());
        assert!(result.is_err());
    }
}
