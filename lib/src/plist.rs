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
        plist_dict.insert(
            "Program".to_string(),
            Value::String(details.program.clone()),
        );
    } else {
        let mut args = vec![Value::String(details.program.clone())];
        args.extend(details.arguments.iter().map(|v| Value::String(v.clone())));
        plist_dict.insert("ProgramArguments".to_string(), Value::Array(args));
    }
    if let Some(wd) = &details.working_directory {
        plist_dict.insert("WorkingDirectory".to_string(), Value::String(wd.clone()));
    }

    // Handle schedule - if scheduled, add StartCalendarInterval instead of RunAtLoad/KeepAlive
    if let Some(schedule) = &details.schedule {
        let mut interval_dict = plist::Dictionary::new();
        for (key, value) in schedule.to_launchd_dict() {
            interval_dict.insert(key, Value::Integer(value.into()));
        }
        plist_dict.insert(
            "StartCalendarInterval".to_string(),
            Value::Dictionary(interval_dict),
        );
    } else {
        // Only add RunAtLoad for non-scheduled services
        if details.run_at_load {
            plist_dict.insert("RunAtLoad".to_string(), Value::Boolean(true));
        }

        if details.keep_alive {
            plist_dict.insert("KeepAlive".to_string(), Value::Boolean(true));
        }
    }

    if !details.env_vars.is_empty() {
        let mut env_dict = plist::Dictionary::new();
        for (key, value) in &details.env_vars {
            env_dict.insert(key.clone(), Value::String(value.clone()));
        }
        plist_dict.insert(
            "EnvironmentVariables".to_string(),
            Value::Dictionary(env_dict),
        );
    }

    let plist_value = Value::Dictionary(plist_dict);

    let mut plist_data = Vec::new();
    plist::to_writer_xml(&mut plist_data, &plist_value).context("Failed to serialize plist")?;
    let plist_string = String::from_utf8(plist_data)?;
    Ok(plist_string)
}
