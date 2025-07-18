#![allow(dead_code)]

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_xml_rs::de::from_str;
use std::ffi::{OsStr, OsString};
use std::fmt::{Display, Write};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::{fs, process::Command};

use crate::temp::{copy_to_temp, create_temp_file};
use crate::utils::get_third_party_binary;

pub fn extract_chapters(mkv_file_path: impl AsRef<Path>) -> anyhow::Result<Chapters> {
    let (_temp_dir, temp_file) =
        create_temp_file("chapters.xml").expect("failed to create temp file");
    let _ = fs::remove_file(&temp_file);

    let tool_path = get_third_party_binary("mkvextract.exe");

    let status = Command::new(tool_path)
        .arg(mkv_file_path.as_ref())
        .arg("chapters")
        .arg(&temp_file)
        .status()?;

    if !status.success() {
        anyhow::bail!(
            "Failed to extract chapters from {}",
            mkv_file_path.as_ref().display()
        );
    }

    let metadata = fs::metadata(&temp_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Chapters not found in {}", mkv_file_path.as_ref().display());
    }

    let xml_content = std::fs::read_to_string(&temp_file)?;
    let chapters = parse_chapter_xml(&xml_content)?;

    Ok(chapters)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Chapters {
    #[serde(rename = "EditionEntry")]
    edition_entry: EditionEntry,
}

impl Chapters {
    pub fn iter(&self) -> impl Iterator<Item = &ChapterAtom> {
        self.edition_entry.chapters.iter()
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ChapterAtom> {
        self.edition_entry.chapters.iter_mut()
    }

    pub fn to_os_string(&self) -> OsString {
        let mut output = String::new();

        for chapter in self {
            let _ = writeln!(
                &mut output,
                "Start: {:<12} End: {:<12} Title: {}",
                chapter.start_time,
                chapter
                    .end_time
                    .clone()
                    .unwrap_or_else(|| "???".to_string()),
                chapter.display.title
            );
        }

        OsString::from(output)
    }
}

impl<'a> IntoIterator for &'a Chapters {
    type Item = &'a ChapterAtom;
    type IntoIter = std::slice::Iter<'a, ChapterAtom>;

    fn into_iter(self) -> Self::IntoIter {
        self.edition_entry.chapters.iter()
    }
}
impl<'a> IntoIterator for &'a mut Chapters {
    type Item = &'a mut ChapterAtom;
    type IntoIter = std::slice::IterMut<'a, ChapterAtom>;

    fn into_iter(self) -> Self::IntoIter {
        self.edition_entry.chapters.iter_mut()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EditionEntry {
    #[serde(rename = "ChapterAtom", default)]
    chapters: Vec<ChapterAtom>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChapterAtom {
    #[serde(rename = "ChapterTimeStart")]
    pub start_time: String,

    #[serde(rename = "ChapterTimeEnd")]
    pub end_time: Option<String>,

    #[serde(rename = "ChapterDisplay")]
    pub display: ChapterDisplay,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChapterDisplay {
    #[serde(rename = "ChapterString")]
    pub title: String,
}

pub fn parse_chapter_xml(xml: &str) -> anyhow::Result<Chapters> {
    let chapters: Chapters = from_str(xml)?;
    Ok(chapters)
}

pub fn read_chapters_from_mkv(mkv_file: impl AsRef<Path>) -> anyhow::Result<Chapters> {
    let chapters = extract_chapters(&mkv_file)?;

    // for chapter in &chapters {
    //     println!(
    //         "File: {}, Start: {}, End: {:?}, Title: {}",
    //         &mkv_file.as_ref().file_name().unwrap().display(),
    //         chapter.start_time,
    //         chapter.end_time,
    //         chapter.display.title
    //     );
    // }

    Ok(chapters)
}

fn chapters_to_xml(chapters: &Chapters) -> anyhow::Result<String> {
    let inner = serde_xml_rs::to_string(chapters)?;
    Ok(format!(r#"<?xml version="1.0"?>\n{}"#, inner))
}

pub fn add_chapter_to_mkv(mkv_file: &str, timestamp: &str, title: &str) -> anyhow::Result<()> {
    let mut chapters = extract_chapters(mkv_file)?;

    let new_chapter = ChapterAtom {
        start_time: timestamp.to_string(),
        end_time: None,
        display: ChapterDisplay {
            title: title.to_string(),
        },
    };
    chapters.edition_entry.chapters.push(new_chapter);

    let xml_output = chapters_to_xml(&chapters)?;

    use std::io::Write;
    let (_temp_dir, temp_file) = create_temp_file("add_chapter_to_mkv_chapters.xml")?;
    let mut file = File::create(&temp_file)?;
    file.write_all(xml_output.as_bytes())?;

    // Step 6: Apply changes via mkvpropedit
    let status = Command::new("third_party/bin/mkvpropedit.exe")
        .arg(mkv_file)
        .arg("--chapters")
        .arg(&temp_file)
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to apply chapters with mkvpropedit");
    }

    Ok(())
}
