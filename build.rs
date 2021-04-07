use std::env;
use std::fs::{create_dir_all, read_dir, File, OpenOptions};
use std::path::Path;
use std::process::{exit, Command, Stdio};

use clap::Shell;

#[path = "src/app.rs"]
mod app;

fn main() {
    // OUT_DIR is set by Cargo and it's where any additional build artifacts
    // are written.
    let outdir = match env::var_os("OUT_DIR") {
        Some(outdir) => outdir,
        None => {
            eprintln!("OUT_DIR environment variable not defined. Please file a bug.");
            exit(1);
        }
    };

    create_dir_all(&outdir).unwrap();
    generate_man_pages(&outdir);

    // Use clap to build completion files.
    macro_rules! gen_completions {
        ($mod:ident, $outdir:expr) => {
            let mut app = app::$mod::args();
            app.gen_completions(app::$mod::NAME, Shell::Bash, $outdir);
            app.gen_completions(app::$mod::NAME, Shell::Fish, $outdir);
        };
    }
    gen_completions!(autorandrd, &outdir);
    gen_completions!(randr_edid, &outdir);
}

fn generate_man_pages<P: AsRef<Path>>(outdir: P) {
    for page in read_dir("man").unwrap() {
        let in_path = page.unwrap().path();
        let out_path = outdir.as_ref().join(in_path.file_stem().unwrap());
        let input = File::open(in_path).unwrap();
        let output = OpenOptions::new()
            .write(true)
            .create(true)
            .open(out_path)
            .unwrap();
        let mut scdoc = Command::new("scdoc")
            .stdin(Stdio::from(input))
            .stdout(output)
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        scdoc.wait().unwrap();
    }
}
