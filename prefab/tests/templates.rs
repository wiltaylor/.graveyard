use std::fs;
use std::path::PathBuf;

use prefab::template::{self, TemplateOption};
use assert_fs::{TempDir, prelude::PathChild};
use assert_fs::prelude::*;

#[test]
fn can_load_template(){
    let template_directory =  TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let template_file = template_directory.child("template/myfile.txt");
    
    template_file.touch().unwrap();

    manifest.write_str(r#"
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    b = { Boolean = { prompt = "Test Boolean", value = true }}
    c = { Regex = { prompt ="Test Regex", pattern = "*", value = "foo" }}
    d = { Choice = { prompt = "Test choice", options = ["a", "b", "c"], value = "a"}}
    e = { Integer = { prompt = "Test int", value = 1 }}
    f = { Float = { prompt = "Test floar", value = 1.0 }}
    "#).unwrap();

    template::Template::load(template_directory.path().clone()).unwrap();
}

#[test]
fn can_generate_static_files_from_template() {
    let template_directory =  TempDir::new().unwrap();
    let target_directory = TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let static_file = template_directory.child("static/myfile.txt");
    let static_file2 = template_directory.child("static/subdir/myfile.txt");

    manifest.touch().unwrap();
    static_file.touch().unwrap();

    manifest.write_str(r#"
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    "#).unwrap();

    static_file.write_str(r#"
    {{a}}
    "#).unwrap();
    
    static_file2.write_str(r#"
    {{a}}
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    template.apply(target_directory.path().clone()).unwrap();

    let mut file1_path = PathBuf::new();
    let mut file2_path = PathBuf::new();

    file1_path.push(&target_directory);
    file1_path.push("myfile.txt");

    file2_path.push(&target_directory);
    file2_path.push("subdir/myfile.txt");

    let file1_string = fs::read_to_string(file1_path).unwrap();
    let file2_string = fs::read_to_string(file2_path).unwrap();

    assert_eq!(file1_string.trim(), "{{a}}");
    assert_eq!(file2_string.trim(), "{{a}}");

   
}

#[test]
fn can_generate_template() {
    let template_directory =  TempDir::new().unwrap();
    let target_directory = TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let template_file = template_directory.child("template/myfile.txt");
    let template_file2 = template_directory.child("template/subdir/myfile.txt");
    
    manifest.touch().unwrap();
    template_file.touch().unwrap();

    manifest.write_str(r#"
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    "#).unwrap();

    template_file.write_str(r#"
    {{a}}
    "#).unwrap();
    
    template_file2.write_str(r#"
    {{a}}
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    template.apply(target_directory.path().clone()).unwrap();

    let mut file1_path = PathBuf::new();
    let mut file2_path = PathBuf::new();

    file1_path.push(&target_directory);
    file1_path.push("myfile.txt");

    file2_path.push(&target_directory);
    file2_path.push("subdir/myfile.txt");

    let file1_string = fs::read_to_string(file1_path).unwrap();
    let file2_string = fs::read_to_string(file2_path).unwrap();

    assert_eq!(file1_string.trim(), "yo");
    assert_eq!(file2_string.trim(), "yo");

}

#[test]
fn can_call_system_in_template() {
    let template_directory =  TempDir::new().unwrap();
    let target_directory = TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let template_file = template_directory.child("template/myfile.txt");
    
    manifest.touch().unwrap();
    template_file.touch().unwrap();

    manifest.write_str(r#"
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    "#).unwrap();

    template_file.write_str(r#"
    {{ system(command="bash -c 'echo hi'") }}
    "#).unwrap();
    
    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    template.apply(target_directory.path().clone()).unwrap();

    let mut file1_path = PathBuf::new();

    file1_path.push(&target_directory);
    file1_path.push("myfile.txt");


    let file1_string = fs::read_to_string(file1_path).unwrap();

    assert_eq!(file1_string.trim(), "hi");

}

#[test]
fn can_replace_template_variables_in_path(){
    let template_directory =  TempDir::new().unwrap();
    let target_directory = TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let template_file = template_directory.child("template/myfile.txt");
    let template_file2 = template_directory.child("template/{{ b }}/myfile.txt");
    let static_file = template_directory.child("static/{{ b }}/myfile-static.txt"); 
    manifest.touch().unwrap();
    template_file.touch().unwrap();

    manifest.write_str(r#"
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    b = { FreeText = { prompt = "Test FreeText", value = "subdir" }}
    "#).unwrap();

    static_file.write_str(r#"
    {{a}}
    "#).unwrap();

    template_file.write_str(r#"
    {{a}}
    "#).unwrap();
    
    template_file2.write_str(r#"
    {{a}}
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    template.apply(target_directory.path().clone()).unwrap();

    let mut file1_path = PathBuf::new();
    let mut file2_path = PathBuf::new();
    let mut static_path = PathBuf::new();

    file1_path.push(&target_directory);
    file1_path.push("myfile.txt");

    file2_path.push(&target_directory);
    file2_path.push("subdir/myfile.txt");

    static_path.push(&target_directory);
    static_path.push("subdir/myfile-static.txt");

    let file1_string = fs::read_to_string(file1_path).unwrap();
    let file2_string = fs::read_to_string(file2_path).unwrap();
    let static_string = fs::read_to_string(static_path).unwrap();

    assert_eq!(file1_string.trim(), "yo");
    assert_eq!(file2_string.trim(), "yo");
    assert_eq!(static_string.trim(), "{{a}}");

   
}

#[test]
fn can_load_config() {
    let template_directory =  TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let template_file = template_directory.child("template/myfile.txt");
    let cfg_file = template_directory.child("config.toml");

    template_file.touch().unwrap();

    manifest.write_str(r#"
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    b = { Boolean = { prompt = "Test Boolean", value = true }}
    c = { Regex = { prompt ="Test Regex", pattern = "*", value = "foo" }}
    d = { Choice = { prompt = "Test choice", options = ["a", "b", "c"], value = "a"}}
    e = { Integer = { prompt = "Test int", value = 1 }}
    f = { Float = { prompt = "Test floar", value = 1.0 }}
    "#).unwrap();

    cfg_file.write_str(r#"
    a = "foo"
    b = true
    c = "xyz"
    d = "b"
    e = 2
    f = 2.5
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    template.load_config(&cfg_file.path()).unwrap();

    let opts = template.get_options();

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

#[test]
fn can_run_before_hook(){
    let template_directory =  TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let before_script = template_directory.child("before.sh");

    let target_directory = TempDir::new().unwrap();
    manifest.write_str(r#"
    before_hook = "sh $PREFAB_TEMPLATE/before.sh"
    
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    b = { Boolean = { prompt = "Test Boolean", value = true }}
    c = { Regex = { prompt ="Test Regex", pattern = ".*", value = "foo" }}
    d = { Choice = { prompt = "Test choice", options = ["a", "b", "c"], value = "a"}}
    e = { Integer = { prompt = "Test int", value = 1 }}
    f = { Float = { prompt = "Test floar", value = 1.0 }}
    "#).unwrap();

    before_script.write_str(r#"
    #!/bin/sh
    echo test > ./test.txt
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    template.apply(target_directory.path().clone()).unwrap();

    let mut result_path = PathBuf::new();

    result_path.push(&target_directory);
    result_path.push("test.txt");

    let result_string = fs::read_to_string(result_path).unwrap();

    assert_eq!(result_string.trim(), "test");
}

#[test]
fn can_run_after_hook(){
    let template_directory =  TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");
    let after_script = template_directory.child("after.sh");

    let target_directory = TempDir::new().unwrap();
    manifest.write_str(r#"
    after_hook = "sh $PREFAB_TEMPLATE/after.sh"
    
    [options]
    a = { FreeText = { prompt = "Test FreeText", value = "yo" }}
    b = { Boolean = { prompt = "Test Boolean", value = true }}
    c = { Regex = { prompt ="Test Regex", pattern = ".*", value = "foo" }}
    d = { Choice = { prompt = "Test choice", options = ["a", "b", "c"], value = "a"}}
    e = { Integer = { prompt = "Test int", value = 1 }}
    f = { Float = { prompt = "Test floar", value = 1.0 }}
    "#).unwrap();

    after_script.write_str(r#"
    #!/bin/sh
    echo test > ./test.txt
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    template.apply(target_directory.path().clone()).unwrap();

    let mut result_path = PathBuf::new();

    result_path.push(&target_directory);
    result_path.push("test.txt");

    let result_string = fs::read_to_string(result_path).unwrap();

    assert_eq!(result_string.trim(), "test");
}

#[test]
fn can_return_error_if_regex_doesnt_match(){
    let template_directory =  TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");

    let target_directory = TempDir::new().unwrap();
    manifest.write_str(r#"
    [options]
    c = { Regex = { prompt ="Test Regex", pattern = "abc", value = "foo" }}
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    let result = template.apply(target_directory.path().clone());

    assert!(result.is_err());
}

#[test]
fn can_return_error_if_choice_doesnt_match(){
    let template_directory =  TempDir::new().unwrap();
    let manifest = template_directory.child("prefab.toml");

    let target_directory = TempDir::new().unwrap();
    manifest.write_str(r#"
    [options]
    d = { Choice = { prompt = "Test choice", options = ["a", "b", "c"], value = "d"}}
    "#).unwrap();

    let mut template = template::Template::load(template_directory.path().clone()).unwrap();
    let result = template.apply(target_directory.path().clone());

    assert!(result.is_err());
}