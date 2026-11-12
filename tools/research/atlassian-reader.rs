use std::{collections::BTreeSet, env, fs};
fn decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}
fn extract(s: &str) -> (String, BTreeSet<String>) {
    let mut text = String::new();
    let mut links = BTreeSet::new();
    let mut at = 0;
    let mut skip = String::new();
    while at < s.len() {
        if s.as_bytes()[at] == b'<' {
            let Some(end) = s[at..].find('>') else { break };
            let tag = &s[at + 1..at + end];
            let lower = tag.to_ascii_lowercase();
            if !skip.is_empty() {
                if lower.starts_with(&format!("/{skip}")) {
                    skip.clear();
                }
            } else if lower.starts_with("script") || lower.starts_with("style") {
                skip = if lower.starts_with("script") {
                    "script"
                } else {
                    "style"
                }
                .into();
            } else {
                if lower.starts_with("a ") {
                    for q in ['\"', '\''] {
                        let key = format!("href={q}");
                        if let Some(i) = tag.find(&key) {
                            let rest = &tag[i + key.len()..];
                            if let Some(j) = rest.find(q) {
                                links.insert(decode(&rest[..j]));
                            }
                        }
                    }
                }
                if ["/p", "/h", "/li", "br", "/div", "/section", "/tr"]
                    .iter()
                    .any(|p| lower.starts_with(p))
                {
                    text.push('\n');
                } else {
                    text.push(' ');
                }
            }
            at += end + 1;
        } else {
            let end = s[at..].find('<').map(|i| at + i).unwrap_or(s.len());
            if skip.is_empty() {
                text.push_str(&s[at..end]);
            }
            at = end;
        }
    }
    (
        decode(&text)
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        links,
    )
}
fn article(s: &str) -> String {
    let start = s.find("<h1").unwrap_or(0);
    let end = s[start..]
        .find("</main>")
        .map(|i| start + i)
        .unwrap_or(s.len());
    extract(&s[start..end]).0
}
fn collect(links_path: &str, out: &str) {
    fs::create_dir_all(out).unwrap();
    let links = fs::read_to_string(links_path).unwrap();
    let urls: Vec<String> = links
        .lines()
        .filter(|p| {
            *p == "/agile/project-management" || p.starts_with("/agile/project-management/")
        })
        .map(|p| format!("https://www.atlassian.com{p}"))
        .collect();
    let mut rows = Vec::new();
    for batch in urls.chunks(6) {
        std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|url| {
                    scope.spawn(move || {
                        let slug = url.rsplit('/').next().unwrap();
                        let html = format!("{out}/{slug}.html");
                        let txt = format!("{out}/{slug}.txt");
                        let output = std::process::Command::new("curl")
                            .args([
                                "--location",
                                "--fail",
                                "--silent",
                                "--show-error",
                                "--max-time",
                                "45",
                                "--output",
                                &html,
                                "--write-out",
                                "%{http_code}\\t%{url_effective}",
                                url,
                            ])
                            .output()
                            .unwrap();
                        let status = String::from_utf8_lossy(&output.stdout).replace("\\t", "\t");
                        if output.status.success() {
                            let s = fs::read_to_string(&html).unwrap();
                            fs::write(&txt, article(&s)).unwrap();
                        }
                        format!(
                            "{slug}\t{url}\t{}\t{status}\t{}",
                            output.status.code().unwrap_or(-1),
                            String::from_utf8_lossy(&output.stderr).replace('\n', " ")
                        )
                    })
                })
                .collect();
            for h in handles {
                rows.push(h.join().unwrap());
            }
        });
    }
    fs::write(format!("{out}/manifest.tsv"), rows.join("\n") + "\n").unwrap();
    println!("Collected {} index article entries into {out}", rows.len());
}
fn main() {
    let args: Vec<_> = env::args().collect();
    if args[1] == "collect" {
        collect(&args[2], &args[3]);
        return;
    }
    let s = fs::read_to_string(&args[1]).expect("read HTML");
    if args.get(2).is_some_and(|x| x == "links") {
        for url in extract(&s).1 {
            println!("{url}");
        }
    } else {
        println!("{}", article(&s));
    }
}
#[test]
fn strips_active_content_and_preserves_unicode() {
    let(t,l)=extract("<h1>Agile café</h1><script>bad()</script><style>bad</style><p>Flow &amp; limits</p><a href=\"/agile/x\">Read</a>");
    assert!(t.contains("Agile café"));
    assert!(t.contains("Flow & limits"));
    assert!(!t.contains("bad"));
    assert!(l.contains("/agile/x"));
}
#[test]
fn links_are_unique() {
    let (_, l) = extract("<a href='/agile/x'>one</a><a href=\"/agile/x\">two</a>");
    assert_eq!(l.len(), 1);
}
