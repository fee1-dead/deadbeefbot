use copypasta::ClipboardProvider;
use copypasta::x11_clipboard::{X11ClipboardContext, Clipboard};

fn main() -> color_eyre::Result<()> {
    let mut clipboard = X11ClipboardContext::<Clipboard>::new().unwrap();
    loop {
        let mut s = String::new();
        std::io::stdin().read_line(&mut s)?;
        let s = s.trim().trim_start_matches('{').trim_end_matches('}');
        let s = s.split('|').collect::<Vec<_>>();
        match s[0] {
            "OnThisDay" | "On this day" => {
                let mut out = String::new();
                for x in s[1..].iter() {
                    let (name, value) = x.split_once('=').unwrap();
                    if let Some(x) = name.strip_prefix("date") {
                        let x = if x.is_empty() { "1" } else { x };
                        out.push_str(&format!("|otd{x}date={value}\n"));
                    } else if let Some(x) = name.strip_prefix("oldid") {
                        let x = if x.is_empty() { "1" } else { x };
                        out.push_str(&format!("|otd{x}oldid={value}\n"));
                    }
                }
                println!("{out}");
                clipboard.set_contents(out).unwrap();
            }
            _ => println!("unrecognized"),
        }
    }
}