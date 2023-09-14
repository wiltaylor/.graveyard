use std::collections::HashMap;



use prefab::config::load_config;
use prefab::template::TemplateOption;
use assert_fs::{TempDir, prelude::PathChild};
use assert_fs::prelude::*;

#[test]
fn can_load_config() {
    let temp_directory =  TempDir::new().unwrap();
    let cfg_file = temp_directory.child("config.toml");

    let mut options: HashMap<String, TemplateOption> = HashMap::new();
    options.insert("a".to_string(), TemplateOption::FreeText { prompt: "Test FreeText".to_string(), value: Some("yo".to_string()) });
    options.insert("b".to_string(), TemplateOption::Boolean { prompt:"Test int".to_string(), value: Some(false)});
    options.insert("c".to_string(), TemplateOption::Regex { prompt:"Test Regex".to_string(), pattern: ".*".to_string(), value: Some("foo".to_string()) });
    options.insert("d".to_string(), TemplateOption::Choice { prompt:"Test choice".to_string(), options: vec!["a".to_string(), "b".to_string(), "C".to_string()], value: Some("a".to_string()) });
    options.insert("e".to_string(), TemplateOption::Integer { prompt:"Test int".to_string(), value: Some(1)});
    options.insert("f".to_string(), TemplateOption::Float { prompt:"Test float".to_string(), value: Some(1.0) });

    cfg_file.write_str(r#"
    a = "foo"
    b = true
    c = "xyz"
    d = "b"
    e = 2
    f = 2.5
    "#).unwrap();

    let opts = load_config(&cfg_file.path(), options).unwrap();

    if let TemplateOption::FreeText { prompt:_, value } = &opts["a"] {
        assert_eq!(value.as_ref().unwrap(), "foo");
    }else{
        assert!(false, "a is not a free text field like expected!");
    }

    if let TemplateOption::Boolean { prompt:_, value } = &opts["b"] {
        assert_eq!(value.as_ref().unwrap(), &true);
    }else{
        assert!(false, "b is not a bool field like expected!");
    }

    if let TemplateOption::Regex { prompt:_, pattern:_, value } = &opts["c"] {
        assert_eq!(value.as_ref().unwrap(), "xyz");
    }else{
        assert!(false, "c is not a regex field like expected!");
    }
    
    if let TemplateOption::Choice{ prompt:_, options:_, value } = &opts["d"] {
        assert_eq!(value.as_ref().unwrap(), "b");
    }else{
        assert!(false, "c is not a choice field like expected!");
    }

    if let TemplateOption::Integer{ prompt:_, value } = &opts["e"] {
        assert_eq!(value.as_ref().unwrap(), &2);
    }else{
        assert!(false, "c is not a choice field like expected!");
    }

    if let TemplateOption::Float{ prompt:_, value } = &opts["f"] {
        assert_eq!(value.as_ref().unwrap(), &2.5);
    }else{
        assert!(false, "c is not a choice field like expected!");
    }
    
}

