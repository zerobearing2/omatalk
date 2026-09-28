//! public/: the landing page's snippets, voices, and assets, and the
//! install.sh / uninstall.sh dispatchers.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use common::{System, TempDir, omatalk, read, repo, run, write};
use scraper::{ElementRef, Html, Selector};

fn public(rel: &str) -> PathBuf {
    repo("public").join(rel)
}

fn host() -> String {
    read(public("CNAME")).trim().to_owned()
}

fn page() -> Html {
    Html::parse_document(&read(public("index.html")))
}

fn select<'a>(root: ElementRef<'a>, css: &str) -> impl Iterator<Item = ElementRef<'a>> {
    let selector = Selector::parse(css).unwrap_or_else(|e| panic!("{css}: {e}"));
    root.select(&selector).collect::<Vec<_>>().into_iter()
}

fn text(element: ElementRef) -> String {
    element.text().collect()
}

/// The `pre` text of each code card that has a copy button: what the
/// copy handler puts on the clipboard.
fn copy_cards(page: &Html) -> Vec<String> {
    select(page.root_element(), ".code-card")
        .filter(|card| select(*card, ".copy-btn").next().is_some())
        .map(|card| text(select(card, "pre").next().expect("code card without pre")))
        .collect()
}

struct Voice {
    src: String,
    seed: String,
    name: String,
    default: bool,
}

fn voices(page: &Html) -> Vec<Voice> {
    select(page.root_element(), "button.voice-btn")
        .map(|button| Voice {
            src: button.attr("data-src").unwrap_or_default().to_owned(),
            seed: select(button, ".voice-avatar")
                .find_map(|a| a.attr("data-seed"))
                .unwrap_or_default()
                .to_owned(),
            name: select(button, ".voice-name").map(text).collect(),
            default: select(button, ".voice-tag").any(|tag| text(tag).trim() == "default"),
        })
        .collect()
}

/// URLs of images, icons, and social preview images the page names.
fn assets(page: &Html) -> Vec<String> {
    let named = [
        ("img", "src"),
        ("link[rel*=icon]", "href"),
        (
            "meta[property='og:image'], meta[name='og:image']",
            "content",
        ),
        (
            "meta[property='twitter:image'], meta[name='twitter:image']",
            "content",
        ),
    ];
    named
        .into_iter()
        .flat_map(|(css, attr)| select(page.root_element(), css).filter_map(move |e| e.attr(attr)))
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Where a page URL lives under public/, or None for another host or a fragment.
fn local_asset(url: &str) -> Option<PathBuf> {
    let absolute = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    if let Some(rest) = absolute {
        let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let hostname = authority.split(':').next().unwrap().to_ascii_lowercase();
        if hostname != host() {
            return None;
        }
        let path = path.split(['?', '#']).next().unwrap();
        let rel = path.trim_start_matches('/');
        return (!rel.is_empty()).then(|| public(rel));
    }
    if url.starts_with('#') {
        return None;
    }
    Some(public(url.trim_start_matches('/')))
}

/// The Daemon's built-in config: `config get` with no config.toml.
fn defaults() -> serde_json::Value {
    let tmp = TempDir::new("site");
    let result = run(
        omatalk(["config", "get", "--json"]).env("OMATALK_CONFIG", tmp.path().join("absent.toml")),
        "",
    );
    assert_eq!(result.code, 0, "{}", result.stderr);
    serde_json::from_str(&result.stdout).unwrap()
}

#[test]
fn copy_buttons_copy_the_visible_snippet() {
    let cards = copy_cards(&page());
    assert_eq!(cards.len(), 5);
    for pre in &cards {
        let text = pre.trim();
        assert!(!text.is_empty());
        assert!(!text.contains("grep -q"), "{text}");
        assert!(!text.contains(">>"), "{text}");
        assert!(!text.contains("hyprctl"), "{text}");
    }

    let bindings = cards.iter().find(|pre| pre.contains("o.bind")).unwrap();
    assert_eq!(
        bindings.trim(),
        r#"o.bind("F8", "Omatalk", "omatalk speak")"#
    );
}

#[test]
fn copy_handler_reads_the_visible_pre() {
    let page = page();
    assert_eq!(select(page.root_element(), "[data-copy]").count(), 0);
    let script: String = select(page.root_element(), "script").map(text).collect();
    assert!(script.contains("pre.textContent"));
    assert!(!script.contains("dataset.copy"));
}

#[test]
fn config_snippet_matches_daemon_defaults() {
    let defaults = defaults();
    // serde_json prints the speed like Python's repr: 1.0, not 1.
    let expected = format!(
        "voice = \"{}\"\nspeed = {}",
        defaults["voice"].as_str().unwrap(),
        defaults["speed"]
    );
    let card = copy_cards(&page())
        .into_iter()
        .find(|pre| pre.contains("voice") && pre.contains("speed"))
        .unwrap();
    assert_eq!(card.trim(), expected);
}

#[test]
fn voice_buttons_match_clips_and_default() {
    let defaults = defaults();
    let voices = voices(&page());
    assert!(!voices.is_empty());
    let page: BTreeSet<&str> = voices.iter().map(|v| v.name.as_str()).collect();
    let disk: BTreeSet<String> = std::fs::read_dir(public("audio/voices"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mp3"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    let disk: BTreeSet<&str> = disk.iter().map(String::as_str).collect();
    assert_eq!(
        page,
        disk,
        "page-only {:?} disk-only {:?}",
        page.difference(&disk).collect::<Vec<_>>(),
        disk.difference(&page).collect::<Vec<_>>()
    );
    for voice in &voices {
        assert_eq!(voice.src, format!("audio/voices/{}.mp3", voice.name));
        assert_eq!(voice.seed, voice.name);
        assert!(public(&voice.src).is_file(), "{}", voice.src);
    }
    let marked: Vec<&str> = voices
        .iter()
        .filter(|v| v.default)
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(marked, [defaults["voice"].as_str().unwrap()]);
}

#[test]
fn install_commands_match_readme_and_cname() {
    let host = host();
    let plugin =
        "omarchy plugin add https://github.com/zerobearing2/omarchy-omatalk-plugin.git --enable"
            .to_owned();
    let install = format!("curl -fsSL https://{host}/install.sh | bash");
    let uninstall = format!("curl -fsSL https://{host}/uninstall.sh | bash");
    let cards = copy_cards(&page());
    let shown: BTreeSet<&str> = cards.iter().map(|c| c.trim()).collect();
    let readme = read(repo("README.md"));
    for command in [&plugin, &install, &uninstall] {
        assert!(shown.contains(command.as_str()), "page lacks {command}");
        assert!(readme.contains(command.as_str()), "README lacks {command}");
    }
    assert!(public("install.sh").is_file());
    assert!(public("uninstall.sh").is_file());
}

#[test]
fn named_assets_exist() {
    let missing: Vec<String> = assets(&page())
        .iter()
        .filter_map(|url| local_asset(url).map(|path| (url, path)))
        .filter(|(_, path)| !path.is_file())
        .map(|(url, path)| format!("{url} -> {}", path.display()))
        .collect();
    assert_eq!(missing, Vec::<String>::new());
}

#[test]
fn dispatcher_fetches_the_latest_release_by_default() {
    for script in ["install.sh", "uninstall.sh"] {
        let sys = System::new();
        let latest = format!("zerobearing2/omatalk/releases/latest/download/{script}");
        write(sys.site(&latest), format!("echo ran latest {script}\n"));

        let result = run(sys.command("bash").arg(public(script)), "");

        assert_eq!(result.code, 0, "{script}: {}", result.stderr);
        assert!(
            result.stdout.contains(&format!("ran latest {script}")),
            "{script}: {}",
            result.stdout
        );
        let downloads = sys.downloads();
        assert_eq!(downloads.len(), 1, "{downloads:?}");
        assert!(
            downloads[0].ends_with(&format!(" https://github.com/{latest}")),
            "{downloads:?}"
        );
    }
}
