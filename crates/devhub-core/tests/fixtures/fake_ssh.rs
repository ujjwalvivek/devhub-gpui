use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

fn main() {
    if let Some(path) = env::var_os("DEVHUB_FAKE_SSH_PID_FILE") {
        fs::write(path, std::process::id().to_string()).unwrap();
    }
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("stdout") => write_bytes(io::stdout(), parse(&args, 1)),
        Some("stderr") => write_bytes(io::stderr(), parse(&args, 1)),
        Some("invalid-utf8") => {
            let count = parse(&args, 1);
            io::stdout().write_all(&vec![0xff; count]).unwrap();
        }
        Some("dual") => {
            write_bytes(io::stdout(), parse(&args, 1));
            write_bytes(io::stderr(), parse(&args, 2));
        }
        Some("stream") => {
            let chunk = parse(&args, 1);
            let count = parse(&args, 2);
            let delay = parse(&args, 3) as u64;
            let mut stdout = io::stdout().lock();
            let bytes = vec![b'x'; chunk];
            for _ in 0..count {
                stdout.write_all(&bytes).unwrap();
                stdout.flush().unwrap();
                thread::sleep(Duration::from_millis(delay));
            }
        }
        Some("sleep") => thread::sleep(Duration::from_millis(parse(&args, 1) as u64)),
        Some("mark") => fs::write(Path::new(&args[1]), b"started").unwrap(),
        Some("exit") => std::process::exit(parse(&args, 1) as i32),
        mode => panic!("unsupported fake SSH mode: {mode:?}"),
    }
}

fn parse(args: &[String], index: usize) -> usize {
    args[index].parse().unwrap()
}

fn write_bytes(writer: impl Write, count: usize) {
    let mut writer = io::BufWriter::new(writer);
    let chunk = [b'x'; 16 * 1024];
    let mut remaining = count;
    while remaining > 0 {
        let write = remaining.min(chunk.len());
        writer.write_all(&chunk[..write]).unwrap();
        remaining -= write;
    }
    writer.flush().unwrap();
}
