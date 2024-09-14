use std::{collections::HashMap, io::{BufRead, BufReader, Cursor, Read}};

use anyhow::{Error, Result};

pub enum SysctlConfigValue{
    String(String),
    SysctlConfig(SysctlConfig),
}

type SysctlConfig = HashMap<String, SysctlConfigValue>;

pub fn load_sysctl(path: String) -> Result<SysctlConfig> {
    let file = std::fs::read_to_string(path)?;
    let r = BufReader::new(Cursor::new(file));
    let r = BufReader::new(r);
    load_sysctl_from_reader(r)
}

fn load_sysctl_from_reader<T: Read>(reader: BufReader<T>) -> Result<SysctlConfig> {
    let mut map = SysctlConfig::new();
    for line in reader.lines() {
        let line = line?;
        insert_entry_of_line(&mut map, line)?;
    }
    Ok(map)
}

fn insert_entry_of_line<'a>(map: &mut SysctlConfig, line: String) -> Result<()> {
    let mut line = line;
    if line.is_empty() {
        return Ok(())
    }

    if line.starts_with("#") || line.starts_with(";") {
        return Ok(())
    }

    let error_or_ignore = if line.starts_with("-") {
        line.remove(0);
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
            m.insert(key.to_string(), SysctlConfigValue::String(value.to_string()));
        } else {
            let next_m = m.entry(key.to_string()).or_insert_with(|| SysctlConfigValue::SysctlConfig(SysctlConfig::new()));
            if let SysctlConfigValue::SysctlConfig(next_m) = next_m {
                m = next_m;
            } else {
                return error_or_ignore("invalid line");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;
    use super::*;

    #[test]
    fn ok() {
        let test_data =
"hoge = fuga
piyo = moge
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string()).unwrap();

        let hoge = map.get("hoge").unwrap();
        if let SysctlConfigValue::String(v) = hoge {
            assert_eq!(v, "fuga");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "hoge");
        }

        let piyo = map.get("piyo").unwrap();
        if let SysctlConfigValue::String(v) = piyo {
            assert_eq!(v, "moge");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "piyo");
        }
    }

    #[test]
    fn ok_with_nested() {
        let test_data =
"foo.bar = bar
foo.baz = baz
bar.baz = foo
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string()).unwrap();

        let foo = map.get("foo").unwrap();
        if let SysctlConfigValue::SysctlConfig(v) = foo {
            let bar = v.get("bar").unwrap();
            if let SysctlConfigValue::String(v) = bar {
                assert_eq!(v, "bar");
            } else {
                panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "foo.bar");
            }

            let baz = v.get("baz").unwrap();
            if let SysctlConfigValue::String(v) = baz {
                assert_eq!(v, "baz");
            } else {
                panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "foo.baz");
            }

        } else {
            panic!("expected SysctlConfigValue::SysctlConfig, but got SysctlConfigValue::String: key={}", "foo");
        }

        let bar = map.get("bar").unwrap();
        if let SysctlConfigValue::SysctlConfig(v) = bar {
            let baz = v.get("baz").unwrap();
            if let SysctlConfigValue::String(v) = baz {
                assert_eq!(v, "foo");
            } else {
                panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "bar.baz");
            }
        } else {
            panic!("expected SysctlConfigValue::SysctlConfig, but got SysctlConfigValue::String: key={}", "bar");
        }
    }

    #[test]
    fn ok_with_comment() {
        let test_data =
"# foo = bar
bar = baz
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string()).unwrap();

        let foo = map.get("foo");
        assert!(foo.is_none());

        let bar = map.get("bar").unwrap();
        if let SysctlConfigValue::String(v) = bar {
            assert_eq!(v, "baz");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "bar");
        }
    }

    #[test]
    fn ok_with_ignore_error() {
        let test_data =
"- foobar
- foo = bar
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string()).unwrap();

        let foo = map.get("foo").unwrap();
        if let SysctlConfigValue::String(v) = foo {
            assert_eq!(v, "bar");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "foo");
        }
    }

    #[test]
    fn ok_with_empty_line() {
        let test_data =
"foo = bar

baz = qux
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string()).unwrap();

        let foo = map.get("foo").unwrap();
        if let SysctlConfigValue::String(v) = foo {
            assert_eq!(v, "bar");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "foo");
        }

        let baz = map.get("baz").unwrap();
        if let SysctlConfigValue::String(v) = baz {
            assert_eq!(v, "qux");
        } else {
            panic!("expected SysctlConfigValue::String, but got SysctlConfigValue::SysctlConfig: key={}", "baz");
        }
    }

    #[test]
    fn ng_with_no_delimiter() {
        let test_data =
"foo
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string());
        assert!(map.is_err());
    }

    #[test]
    fn ng_with_zero_length_key() {
        let test_data =
" = foo
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string());
        assert!(map.is_err());
    }

    #[test]
    fn ng_with_zero_length_value() {
        let test_data =
"foo =
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string());
        assert!(map.is_err());
    }

    #[test]
    fn ng_with_whitespace_key() {
        let test_data =
"foo bar = baz
";

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(test_data.as_bytes()).unwrap();

        let map = load_sysctl(f.path().to_str().unwrap().to_string());
        assert!(map.is_err());
    }
}
