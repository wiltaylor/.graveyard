use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf}, env::current_dir,
};

use anyhow::{anyhow, Ok, Result};
use serde::Deserialize;
use tera::{Context, Tera, from_value, to_value};

use crate::util::{copy_all, exec};
use std::result::{Result::Ok as Ok2, Result::Err as Err2};
use regex::Regex as re;

#[derive(Deserialize, Debug, Clone)]
pub enum TemplateOption {
    FreeText {
        prompt: String,
        value: Option<String>,
    },
    Boolean {
        prompt: String,
        value: Option<bool>,
    },
    Integer {
        prompt: String,
        value: Option<i64>,
    },
    Float {
        prompt: String,
        value: Option<f64>,
    },
    Regex {
        prompt: String,
        pattern: String,
        value: Option<String>,
    },
    Choice {
        prompt: String,
        options: Vec<String>,
        value: Option<String>,
    },
}

#[derive(Deserialize, Debug)]
pub struct Manifest {
    pub title: Option<String>,
    #[allow(dead_code)]
    pub author: Option<String>,
    #[allow(dead_code)]
    pub description: Option<String>,
    before_hook: Option<String>,
    after_hook: Option<String>,
    options: HashMap<String, TemplateOption>,
}

#[derive(Debug)]
pub struct Template {
    tera: Tera,
    pub manifest: Manifest,
    pub source_path: PathBuf,
}

impl TemplateOption {
    pub fn set_value(self, text: String)-> TemplateOption {
        match self {
            TemplateOption::FreeText { prompt, value:_ } => {
                if text != "".to_string() {
                    TemplateOption::FreeText { prompt, value: Some(text) }
                }else{
                    TemplateOption::FreeText { prompt, value: None }
                }
            },
            TemplateOption::Boolean { prompt, value:_ } => {
                let text = text.trim().to_lowercase();
                if text == "true" {
                    return TemplateOption::Boolean { prompt, value: Some(true) }
                }

                if text == "" {
                    return TemplateOption::Boolean { prompt, value: None };
                }

                TemplateOption::Boolean { prompt, value: Some(false) }
            },
            TemplateOption::Integer { prompt, value } => {
                let result: Result<i64, _> = text.parse();

                if text == "" {
                    return TemplateOption::Integer { prompt, value: None };
                }

                 match result {
                    Ok2(num) => {
                        TemplateOption::Integer { prompt, value: Some(num)}
                    },
                    Err2(_) => {
                        TemplateOption::Integer { prompt, value }
                    },
                }                
            },
            TemplateOption::Float { prompt, value } => {
                let mut text = text;
                if text.ends_with(".") {
                    text += "0";
                }

                let result: Result<f64, _> = text.parse();

                if text == "" {
                    return TemplateOption::Float { prompt, value: None };
                }

                 match result {
                    Ok2(num) => {
                        TemplateOption::Float { prompt, value: Some(num)}
                    },
                    Err2(_) => {
                        TemplateOption::Float { prompt, value: value }
                    },
                }  
            },
            TemplateOption::Regex { prompt, pattern, value:_ } => {
                if text != "".to_string() {
                    TemplateOption::Regex { prompt, pattern, value: Some(text) }
                }else{
                    TemplateOption::Regex { prompt, pattern, value: None }
                }
            },
            TemplateOption::Choice { prompt, options, value:_ } => {
                if text != "".to_string() {
                    TemplateOption::Choice { prompt, options, value: Some(text) }
                }else{
                    TemplateOption::Choice { prompt, options, value: None }
                }
            },
        }       
    }
}

impl Manifest { 
    fn validate_options(&self) -> Result<()> {

        for (k, v) in &self.options {
            match v {
                TemplateOption::Regex { prompt:_, pattern, value } => {

                    if let Some(value) = value {
                    let regex = re::new(pattern)?;

                        if !regex.is_match(value) {
                            return Err(anyhow!("Regular expression for {} didn't match!", k));
                        }
                    }
                },
                TemplateOption::Choice { prompt:_, options, value } => {
                    if let Some(value) = value {
                        if !options.contains(value) {
                            return Err(anyhow!("Option not in the choice list for {} was set.", k));
                        }
                    }
                },
                _ => ()
            }
        }

        Ok(())
    }
}

fn system(args: &HashMap<String, tera::Value>) -> Result<tera::Value, tera::Error> {

    if let Some(command) = args.get("command"){
        if let Ok2(command) = from_value::<String>(command.clone()){
            if let Ok2(result) = exec(command.as_str(), HashMap::<String, String>::new(), current_dir()?){
                return Ok2(to_value::<String>(result)?);
            }else{
                return Err(tera::Error::msg("Failed to run command!"));
            }   
        }
    }
    
    return Err2(tera::Error::msg("You need to pass in the command argument!"));

}

impl Template {
    fn render_file_path(&mut self, path: impl AsRef<Path>) -> Result<String> {
        let ctx = self.get_context();
        if let Some(path) = path.as_ref().clone().to_str(){
            return Ok(self.tera.render_str(&path, &ctx)?);
        }else{
            return Err(anyhow!("Failed to process path!"));
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Template> {
        let mut manifestpath = PathBuf::new();
        let mut tera = Tera::default();

        manifestpath.push(&path);
        manifestpath.push("prefab.toml");

        let mut template_folder_path = PathBuf::new();
        template_folder_path.push(&path);
        template_folder_path.push("template");

        if !manifestpath.exists() {
            return Err(anyhow!(
                "Could not find a template manifest at template path {:?}",
                &manifestpath
            ));
        }

        let file = fs::read_to_string(&manifestpath)?;
        let toml = toml::from_str::<Manifest>(&file)?;

        if template_folder_path.exists() {
            //Add file glob so we find all items in template
            template_folder_path.push("**");

            let teradir = template_folder_path.as_mut_os_str().to_str().unwrap();
            tera = Tera::new(teradir)?;
        }

        tera.register_function("system", system);

        Ok(Template {
            tera: tera,
            manifest: toml,
            source_path: path.as_ref().to_path_buf(),
        })
    }

    fn get_context(&self) -> Context {
        let mut ctx = Context::new();

        for (name, opt) in &self.manifest.options {
            match opt {
                TemplateOption::FreeText { prompt: _, value } => {
                    if let Some(val) = value {
                        ctx.insert(name, val);
                    }
                }
                TemplateOption::Boolean { prompt: _, value } => {
                    if let Some(val) = value {
                        ctx.insert(name, val);
                    }
                }
                TemplateOption::Integer { prompt: _, value } => {
                    if let Some(val) = value {
                        ctx.insert(name, val);
                    }
                }
                TemplateOption::Float { prompt: _, value } => {
                    if let Some(val) = value {
                        ctx.insert(name, val);
                    }
                }
                TemplateOption::Regex {
                    prompt: _,
                    pattern: _,
                    value,
                } => {
                    if let Some(val) = value {
                        ctx.insert(name, val);
                    }
                }
                TemplateOption::Choice {
                    prompt: _,
                    options: _,
                    value,
                } => {
                    if let Some(val) = value {
                        ctx.insert(name, val)
                    }
                }
            }
        }

        ctx
    }

    pub fn apply(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let ctx = self.get_context();
        let names: Vec<String> = self.tera.get_template_names().map(|n| n.to_string()).collect();

        self.manifest.validate_options()?;

        if let Some(hook) = &self.manifest.before_hook {
            let mut env = HashMap::<String, String>::new();
            env.insert("PREFAB_TEMPLATE".to_string(), self.source_path.to_string_lossy().to_string());
            exec(&hook, env, &path)?;
        }

        for tmpl in names {
            let text = self.tera.render(&tmpl, &ctx)?;
            let mut file_name = PathBuf::new();

            file_name.push(&path);
            file_name.push(tmpl);

            let rendered_path = self.render_file_path(file_name)?;
            file_name = PathBuf::new();
            file_name.push(rendered_path);

            let mut folder = file_name.clone();
            folder.pop();
            fs::create_dir_all(folder)?;

            println!("Creating {:?}", &file_name);

            fs::write(file_name, text)?;
        }

        let mut static_folder_path = PathBuf::new();
        static_folder_path.push(&self.source_path);
        static_folder_path.push("static");

        if static_folder_path.exists() {
            let ctx = self.get_context();
            copy_all(static_folder_path, &path, &mut self.tera, &ctx)?;
        }

        if let Some(hook) = &self.manifest.after_hook {
            let mut env = HashMap::<String, String>::new();
            env.insert("PREFAB_TEMPLATE".to_string(), self.source_path.to_string_lossy().to_string());
            exec(&hook, env, &path)?;
        }

        Ok(())
    }

    pub fn get_options(&self) -> HashMap<String, TemplateOption> {
        let mut result = HashMap::new();
        for (key, value) in self.manifest.options.iter() {
            result.insert(key.clone(), value.clone());
        }

        result
    }

    pub fn set_options(&mut self, options: HashMap<String, TemplateOption>) {
        self.manifest.options = options;
    }
}
