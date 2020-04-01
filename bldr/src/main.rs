#[macro_use]
extern crate serde_derive;


use clap::{Arg, App, SubCommand};
use glob::glob;
use std::process::Command;

mod graph;

use graph::{load_actions, Document, ActionType, Folder};

fn main() {

    let matches = App::new("bldr")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Wil Taylor bldr@wiltaylor.dev")
        .about("Build toolchain managment tool")

        .subcommand(SubCommand::with_name("init")
            .about("Build toolchain images."))

        .subcommand(SubCommand::with_name("destroy")
             .about("Destroy toolchain to free up resources."))

        .subcommand(SubCommand::with_name("ls")
             .about("List all actions"))

        .arg(Arg::with_name("action")
             .help("Action to execute with parameters.")
             .min_values(1)
             .index(1)
        )


        .get_matches();


    let doc = load_actions("./bldr.yaml".to_string()).expect("Failed to open bldr.yaml file!");

    if let Some(_command) = matches.subcommand_matches("init") {
        do_init("./bldr".to_string(), &doc);
    }

    if let Some(_command) = matches.subcommand_matches("destroy") {
        do_destroy("./bldr".to_string(), &doc);
    }

    if let Some(_command) = matches.subcommand_matches("ls") {
        do_ls(&doc);
    }

    if let Some(actions) = matches.values_of_lossy("action") {
        do_action(&actions[0], &actions[1..], &doc);
    }

}

fn do_init(path: String, doc: &Document) {
    let glob_pattern = path + "/Dockerfile-*";

    for entry in glob(&glob_pattern).expect("Failed to read glob data!") {
        match entry {
            Ok(data) => {

                let tag_name = str::replace(data.file_name().unwrap().to_str().unwrap(), "Dockerfile-", "");

                Command::new("docker")
                    .arg("build")
                    .arg("-f")
                    .arg(data.to_str().unwrap())
                    .arg("--tag")
                    .arg(doc.name.as_str().to_owned() + ":" + &tag_name)
                    .arg(".")
                    .spawn()
                    .expect("Docker failed to start!")
                    .wait().expect("Docker command didn't run as expected!");
            },
            Err(e) => panic!("Error finding docker files!: {:?}", e)
        }
    }
}

fn do_destroy(path: String, doc: &Document) {
     let glob_pattern = path + "/Dockerfile-*";

    for entry in glob(&glob_pattern).expect("Failed to read glob data!") {
        match entry {
            Ok(data) => {

                let tag_name = str::replace(data.file_name().unwrap().to_str().unwrap(), "Dockerfile-", "");

                Command::new("docker")
                    .arg("image")
                    .arg("rm")
                    .arg("-f")
                    .arg(doc.name.as_str().to_owned() + ":" + &tag_name)
                    .spawn()
                    .expect("Docker failed to start!")
                    .wait().expect("Docker command didn't run as expected!");
            },
            Err(e) => panic!("Error finding docker files!: {:?}", e)
        }
    }
}

fn do_ls(doc: &Document) {
    println!("Actions: ");

    for act in &doc.actions {

        if act.description != "" {
            println!("{} - {}", act.name, act.description);
        } else {
            println!("{}", act.name);
        }
    }

    println!("");
    println!("Built in actions:");
    println!("init - build all toolchain container images.");
    println!("destroy - delete all toolchain images.");
    println!("ls - This list command.");

    println!("");
}

fn do_action(action: &String, args: &[String], doc: &Document) {

    println!("Running action {}", &action);

    for act in &doc.actions {
        if act.name.to_lowercase() == action.to_lowercase() {

            if act.depend.len() != 0 {
                for dep in &act.depend {
                    do_action(dep, &[], &doc);
                }
            }

            match act.act_type {
                ActionType::Meta => (), //This is used for actions that don't do anything except dpend on other actions.
                ActionType::OneShot => {
                    let mut cmd = Command::new("docker");

                    cmd.arg("run")
                       .arg("--rm")
                       .arg("-t");

                    if act.net != "" {
                        cmd.arg("--net")
                           .arg(&act.net);
                    }

                    for folder in &doc.folders {

                        println!("Mapping folder {} to {} in container", &folder.host_path, &folder.virt_path);

                        cmd.arg("-v")
                           .arg(folder.host_path.to_owned() + ":" + &folder.virt_path);
                    }

                    for folder in &act.folders {

                        println!("Mapping folder {} to {} in container", &folder.host_path, &folder.virt_path);


                        cmd.arg("-v")
                           .arg(folder.host_path.to_owned() + ":" + &folder.virt_path) ;
                    }

                    if act.working_dir != "" {

                        println!("Setting working directory to {} in container.", &act.working_dir);

                        cmd.arg("--workdir")
                           .arg(&act.working_dir);
                    }

                    cmd.arg(&act.image);
                    cmd.arg(&act.command);


                    if act.args.len() != 0 {
                        cmd.args(&act.args);
                    }

                    if args.len() != 0 {
                        cmd.args(args.clone());
                    }

                    cmd.spawn().expect("Failed to run host command!")
                               .wait().expect("Failed to wait for host command!");

                    fix_folders(&act.folders);
                    fix_folders(&doc.folders);
                },
                ActionType::Persist => {
                     let mut cmd = Command::new("docker");

                    cmd.arg("run")
                       .arg("-d");

                    if act.net != "" {
                        cmd.arg("--net")
                           .arg(&act.net);
                    }

                    for folder in &doc.folders {
                        cmd.arg("-v")
                           .arg(folder.host_path.to_owned() + ":" + &folder.virt_path);
                    }

                    for folder in &act.folders {
                        cmd.arg("-v")
                           .arg(folder.host_path.to_owned() + ":" + &folder.virt_path) ;
                    }

                    if act.working_dir != "" {
                        cmd.arg("--workdir")
                           .arg(&act.working_dir);
                    }

                    cmd.arg(&act.image);
                    cmd.arg(&act.command);


                    if act.args.len() != 0 {
                        cmd.args(&act.args);
                    }

                    if args.len() != 0 {
                        cmd.args(args.clone());
                    }

                    cmd.spawn().expect("Failed to run host command!")
                       .wait().expect("Failed to wait for host command!");
                },
                ActionType::Host => {
                    let mut cmd = Command::new(&act.command);

                    if act.args.len() != 0 {
                        cmd.args(&act.args);
                    }

                    if args.len() != 0 {
                        cmd.args(args.clone());
                    }

                    cmd.spawn().expect("Failed to run host command!")
                       .wait().expect("Failed to wait for host command!");

                },
                ActionType::Kill => {
                    let output = Command::new("docker")
                        .arg("ps")
                        .arg("-q")
                        .arg("--filter")
                        .arg("ancestor=".to_owned() + &act.image)
                        .output()
                        .expect("Failed to query docker for running containers!");

                    let full_text = String::from_utf8_lossy(&output.stdout);
                    let text = full_text.split("\n");

                    for line in text {
                        Command::new("docker")
                            .arg("stop")
                            .arg(line)
                            .spawn().expect("Failed to terminate docker container!")
                            .wait().expect("Failed to wait for docker container termination!");

                        Command::new("docker")
                            .arg("rm")
                            .arg(line)
                            .spawn().expect("Failed to remove container!")
                                    .wait().expect("Failed to wait for container remove command!");
                    }

                    fix_folders(&act.folders);
                    fix_folders(&doc.folders);

                }
            }

        }
    }
}

fn fix_folders(folders: &Vec<Folder>) {

    //Not a problem on windows so exit.
    if cfg!(windows) {
        return;
    }

    let uid_output = Command::new("id")
        .arg("-u")
        .output()
        .expect("Failed to get user id");

    let gid_output = Command::new("id")
        .arg("-g")
        .output()
        .expect("Failed to get group id");

    let uid = String::from_utf8_lossy(&uid_output.stdout).to_string();
    let gid = String::from_utf8_lossy(&gid_output.stdout).to_string();

    for folder in folders {
        if !folder.no_fix {
            Command::new("docker")
                .arg("run")
                .arg("--rm")
                .arg("-v")
                .arg(folder.host_path.to_owned() + ":" + &folder.virt_path)
                .arg("bash")
                .arg("chown")
                .arg("-R")
                .arg(uid.to_owned() + ":" + &gid)
                .arg(&folder.virt_path)
                .output().expect("Failed to chown folder.");
        }
    }
}
