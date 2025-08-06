use crate::ServiceDetails;
use anyhow::{Context, Result};
use plist::Value;

pub fn parse_plist(_content: &str) -> Result<ServiceDetails> {
    unimplemented!()
}

pub fn generate_file(details: &ServiceDetails) -> Result<String> {
    let mut plist_dict = plist::Dictionary::new();

    plist_dict.insert("Label".to_string(), Value::String(details.name.clone()));

    if details.arguments.is_empty() {
        plist_dict.insert("Program".to_string(), Value::String(details.program.clone()));
    } else {
        let mut args = vec![Value::String(details.program.clone())];
        args.extend(details.arguments.iter().map(|v| Value::String(v.clone())));
        plist_dict.insert("ProgramArguments".to_string(), Value::Array(args));
    }
    if let Some(wd) = &details.working_directory {
        plist_dict.insert("WorkingDirectory".to_string(), Value::String(wd.clone()));
    }

    if details.run_at_load {
        plist_dict.insert("RunAtLoad".to_string(), Value::Boolean(true));
    }

    if details.keep_alive {
        plist_dict.insert("KeepAlive".to_string(), Value::Boolean(true));
    }

    let plist_value = Value::Dictionary(plist_dict);

    // Create the plist file in user's LaunchAgents directory
    
    // Write the plist file
    let mut plist_data = Vec::new();
    plist::to_writer_xml(&mut plist_data, &plist_value).context("Failed to serialize plist")?;
    String::from_utf8(plist_data).map_err(Into::into)
}

