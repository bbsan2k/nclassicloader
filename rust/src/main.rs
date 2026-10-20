use std::{env, fs, process};

fn main() {
    if let Err(error) = run() {
        eprintln!("clover-rust: {error}");
        #[cfg(feature = "native-app")]
        if env::args_os().len() == 1 {
            let _ = sdl3::messagebox::show_simple_message_box(
                sdl3::messagebox::MessageBoxFlag::ERROR,
                "Classic Menu Rust",
                &error.to_string(),
                None,
            );
        }
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().collect();
    match args.get(1).and_then(|arg| arg.to_str()) {
        #[cfg(feature = "native-app")]
        None => clover_portable::ui::run(&[]).map_err(Into::into),
        #[cfg(feature = "native-app")]
        Some("window-smoke") if args.len() == 2 => {
            clover_portable::ui::smoke_test().map_err(Into::into)
        }
        #[cfg(not(feature = "native-app"))]
        Some("window-smoke") if args.len() == 2 => Err("window-smoke requires `--features native-app`".into()),
        #[cfg(feature = "native-app")]
        Some("run") => clover_portable::ui::run(&args[2..]).map_err(Into::into),
        #[cfg(not(feature = "native-app"))]
        Some("run") => Err("run requires `--features native-app`".into()),
        Some("convert-sfrom") if args.len() == 4 => {
            let input = fs::read(&args[2])?;
            let conversion = clover_portable::sfrom::convert_sfrom(&input)?;
            fs::write(&args[3], &conversion.rom)?;
            println!("{}", serde_json::to_string_pretty(&conversion.metadata)?);
            Ok(())
        }
        Some("inspect-image") if args.len() == 3 => {
            let files = clover_portable::image_reader::read_image(std::path::Path::new(&args[2]))?;
            let summary = serde_json::json!({
                "selected_files": files.len(),
                "files": files.iter().map(|(path, bytes)| (path, bytes.len())).collect::<std::collections::BTreeMap<_, _>>(),
            });
            println!("{}", serde_json::to_string_pretty(&summary)?);
            Ok(())
        }
        Some("import-image") if args.len() == 4 => {
            let profile = clover_portable::profile::import_image(
                std::path::Path::new(&args[2]),
                std::path::Path::new(&args[3]),
                Some(&mut |message| eprintln!("{message}")),
            )?;
            println!("{}", serde_json::to_string_pretty(&profile.manifest)?);
            Ok(())
        }
        Some("list-profiles") if args.len() == 3 => {
            let profiles = clover_portable::profile::list_profiles(std::path::Path::new(&args[2]));
            let output: Vec<_> = profiles.iter().map(|profile| serde_json::json!({
                "directory": profile.directory,
                "label": profile.label(),
                "region": profile.manifest.get("region"),
                "version": profile.manifest.get("version"),
            })).collect();
            println!("{}", serde_json::to_string_pretty(&output)?);
            Ok(())
        }
        _ => Err(
            "usage: clover-rust convert-sfrom INPUT.sfrom OUTPUT.sfc | inspect-image IMAGE.hsqs | import-image IMAGE CACHE | list-profiles CACHE | run --image IMAGE | run --assets DIR | window-smoke"
                .into(),
        ),
    }
}
