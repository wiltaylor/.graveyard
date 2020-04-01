use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum ActionType {
    OneShot,
    Persist,
    Host,
    Kill,
    Meta
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Folder {
    pub host_path: String,
    pub virt_path: String,

    #[serde(default)]
    pub no_fix: bool
   
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Document {
    pub name: String,

    #[serde(default)]
    pub description: String,

    #[serde(default)]
    pub folders: Vec<Folder>,

    pub actions: Vec<Action>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    pub name: String,

    #[serde(default)]
    pub description: String,

    pub act_type: ActionType,

    #[serde(default)]
    pub image: String,

    #[serde(default)]
    pub net: String,

    #[serde(default)]
    pub folders: Vec<Folder>,

    #[serde(default)]
    pub depend: Vec<String>,

    #[serde(default)]
    pub command: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default)]
    pub working_dir: String
}

pub fn load_actions(path: String) -> Result<Document, Error> {
    let abs_path = fs::canonicalize(path).unwrap();

    if let Ok(yaml) = fs::read_to_string(abs_path) {

        match serde_yaml::from_str::<Document>(yaml.as_str()) {
            Ok(mut data) => {
                for folder in &mut data.folders {
                    let srcdir = PathBuf::from(&folder.host_path);
                    folder.host_path = fs::canonicalize(srcdir).unwrap().to_string_lossy().to_string();
                }

                for act in &mut data.actions {
                    for folder in &mut act.folders {
                        let srcdir = PathBuf::from(&folder.host_path);
                        folder.host_path = fs::canonicalize(srcdir).unwrap().to_string_lossy().to_string();
                    }
                }
                return Ok(data);
            },
            Err(e) => panic!("Failed to get yaml data {:?}", e)
        }
    }

    Err(Error::new(ErrorKind::Other, "Failed to retrive data from file!"))

}
