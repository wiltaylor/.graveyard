use std::{path::Path, fs, process::Command, str::from_utf8, collections::HashMap};
use anyhow::{Ok, Result};
use tera::{Tera, Context};

pub fn copy_all(source: impl AsRef<Path>, destination: impl AsRef<Path>, tera: &mut Tera, ctx: &Context) -> Result<()>
{

    let dir = fs::read_dir(&source)?;

    if !destination.as_ref().exists() {
        fs::create_dir_all(&destination)?;
    }

    for entry in dir {
        let e = entry?;
        let templated_name = tera.render_str(e.file_name().to_str().as_ref().unwrap(),ctx)?;
        if e.file_type()?.is_file() {
            fs::copy(e.path(), &destination.as_ref().join(&templated_name))?;
        }

        if e.file_type()?.is_dir() {
            copy_all(&source.as_ref().join(e.file_name()), destination.as_ref().join(&templated_name), tera, ctx)?;
        }
    }

    Ok(())
}

pub fn exec(command: &str, environment: HashMap<String, String>, working_directory: impl AsRef<Path>) -> Result<String>{
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
                .current_dir(working_directory)
                .envs(environment)
                .args(["/C", command])
                .output()
                .expect("failed to execute process")
    } else {
        Command::new("sh")
                .envs(environment)
                .current_dir(working_directory)
                .arg("-c")
                .arg(command)
                .output()?
    };

    let output = from_utf8(&output.stdout)?;
    return Ok(output.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::{TempDir, prelude::PathChild};
    use assert_fs::prelude::*; 

   #[test]
   fn can_copy_directory_contents_to_other_directory(){
        let src_dir = TempDir::new().unwrap();
        let dst_dir = TempDir::new().unwrap();
        let mut tera = Tera::default();
        let ctx = Context::default();

        let filea= src_dir.child("child1.txt");
        let fileb = src_dir.child("subdir/child2.txt");

        filea.write_str("FileA").unwrap();
        fileb.write_str("FileB").unwrap();

        copy_all(src_dir.path(), dst_dir.path(),&mut tera,&ctx).unwrap();

        let filea_dest = dst_dir.child("child1.txt");
        let fileb_dest = dst_dir.child("subdir/child2.txt");
        
        assert!(filea_dest.exists());
        assert!(fileb_dest.exists());
        

   } 
}

