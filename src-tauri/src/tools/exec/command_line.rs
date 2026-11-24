//! Platform-aware command-line splitter shared by policy validation and execution.

pub(crate) fn split_command(command: &str) -> Result<Vec<String>, &'static str> {
    #[cfg(windows)]
    {
        split_windows_command(command)
    }
    #[cfg(not(windows))]
    {
        shell_words::split(command).map_err(|_| "Invalid command syntax")
    }
}

/// Windows CRT quoting: backslashes are literal unless immediately before a
/// double quote. No shell expansion; single quotes are ordinary characters.
#[cfg(any(windows, test))]
pub(crate) fn split_windows_command(command: &str) -> Result<Vec<String>, &'static str> {
    let mut chars = command.chars().peekable();
    let mut args = Vec::new();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() && !quoted {
                break;
            }
            let mut slashes = 0;
            while chars.peek() == Some(&'\\') {
                chars.next();
                slashes += 1;
            }
            if chars.peek() == Some(&'"') {
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                chars.next();
                if slashes % 2 == 1 {
                    arg.push('"');
                } else if quoted && chars.peek() == Some(&'"') {
                    chars.next();
                    arg.push('"');
                } else {
                    quoted = !quoted;
                }
            } else {
                arg.extend(std::iter::repeat_n('\\', slashes));
                match chars.peek() {
                    Some(c) if c.is_whitespace() && !quoted => break,
                    Some(_) => {
                        if let Some(next_char) = chars.next() {
                            arg.push(next_char);
                        }
                    }
                    None => break,
                }
            }
        }
        if quoted {
            return Err("Unclosed double quote in Windows command");
        }
        args.push(arg);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_quotes_and_empty_arguments_round_trip() {
        assert_eq!(
            split_windows_command(
                r#"D:\env\python.exe "D:\project files\test.py" "" C:\plain\path"#
            )
            .unwrap(),
            [
                r"D:\env\python.exe",
                r"D:\project files\test.py",
                "",
                r"C:\plain\path"
            ]
        );
        assert_eq!(
            split_windows_command(
                r#""C:\Program Files\Python\python.exe" -c "print('ok')" "C:\folder\\" "a\"b""#
            )
            .unwrap(),
            [
                r"C:\Program Files\Python\python.exe",
                "-c",
                "print('ok')",
                r"C:\folder\",
                "a\"b"
            ]
        );
        assert!(split_windows_command("python \"unfinished").is_err());
    }
}
