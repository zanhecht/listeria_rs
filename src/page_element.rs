use crate::listeria_list::ListeriaList;
use crate::listeria_page::ListeriaPage;
use crate::render_wikitext::RendererWikitext;
use crate::renderer::Renderer;
use crate::template::Template;
use anyhow::Result;
use regex::Regex;
use regex::RegexBuilder;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PageElement {
    before: String,
    template_start: String,
    _inside: String,
    template_end: String,
    after: String,
    list: ListeriaList,
    is_just_text: bool,
}

impl PageElement {
    pub fn new_from_text(text: &str, page: &ListeriaPage) -> Option<Self> {
        let start_template = page
            .config()
            .get_local_template_title_start(&page.wiki())
            .ok()?;
        let end_template = page
            .config()
            .get_local_template_title_end(&page.wiki())
            .ok()?;
        let pattern_string_start = r#"\{\{(Wikidata[ _]list[^\|]*|"#.to_string()
            + &start_template.replace(" ", "[ _]")
            //+ r#")\s*\|"#; // New version
            + r#"[^\|]*)"#; // Orig
                            //+ r#")"#;
        let pattern_string_end = r#"\{\{(Wikidata[ _]list[ _]end|"#.to_string()
            + &end_template.replace(" ", "[ _]")
            + r#")(\s*\}\})"#;
        let seperator_start: Regex = RegexBuilder::new(&pattern_string_start)
            .multi_line(true)
            .dot_matches_new_line(true)
            .case_insensitive(true)
            .build()
            .ok()?;
        let seperator_end: Regex = RegexBuilder::new(&pattern_string_end)
            .multi_line(true)
            .dot_matches_new_line(true)
            .case_insensitive(true)
            .build()
            .ok()?;

        let match_start = match seperator_start.find(&text) {
            Some(m) => m,
            None => return None,
        };

        let (match_end, single_template) = match seperator_end.find_at(&text, match_start.start()) {
            Some(m) => (m, false),
            None => (match_start, true), // No end template, could be tabbed data
        };

        let remaining = if single_template {
            String::from_utf8(text.as_bytes()[match_start.end()..].to_vec()).ok()?
        } else {
            if match_end.start() < match_start.end() {
                return None;
            }
            String::from_utf8(text.as_bytes()[match_start.end()..match_end.start()].to_vec())
                .ok()?
        };
        let template_start_end_bytes = match Self::get_template_end(remaining) {
            Some(pos) => pos + match_start.end(),
            None => return None,
        };
        let inside = if single_template {
            String::new()
        } else {
            String::from_utf8(text.as_bytes()[template_start_end_bytes..match_end.start()].to_vec())
                .ok()?
        };

        let template = Template::new_from_params(
            "".to_string(),
            String::from_utf8(
                text.as_bytes()[match_start.end()..template_start_end_bytes - 2].to_vec(),
            )
            .ok()?,
        ).ok()?;

        Some(Self {
            before: String::from_utf8(text.as_bytes()[0..match_start.start()].to_vec()).ok()?,
            template_start: String::from_utf8(
                text.as_bytes()[match_start.start()..template_start_end_bytes].to_vec(),
            )
            .ok()?,
            _inside: inside,
            template_end: if single_template {
                String::new()
            } else {
                String::from_utf8(text.as_bytes()[match_end.start()..match_end.end()].to_vec())
                    .ok()?
            },
            after: String::from_utf8(text.as_bytes()[match_end.end()..].to_vec()).ok()?,
            list: ListeriaList::new(template, page.page_params()),
            is_just_text: false,
        })
    }

    pub fn new_just_text(text: &str, page: &ListeriaPage) -> Self {
        let template = Template {
            title: String::new(),
            params: HashMap::new(),
        };
        Self {
            before: text.to_string(),
            template_start: String::new(),
            _inside: String::new(),
            template_end: String::new(),
            after: String::new(),
            list: ListeriaList::new(template, page.page_params()),
            is_just_text: true,
        }
    }

    pub fn get_and_clean_after(&mut self) -> String {
        let ret = self.after.clone();
        self.after = String::new();
        ret
    }

    pub fn new_inside(&self) -> Result<String> {
        match self.is_just_text {
            true => Ok(String::new()),
            false => {
                let mut renderer = RendererWikitext::new();
                renderer.render(&self.list)
            }
        }
    }

    pub fn as_wikitext(&self) -> Result<String> {
        match self.is_just_text {
            true => Ok(self.before.clone()),
            false => Ok(self.before.clone()
                + &self.template_start
                + "\n"
                + &self.new_inside()?
                + "\n"
                + &self.template_end
                + &self.after),
        }
    }

    pub async fn process(&mut self) -> Result<()> {
        match self.is_just_text {
            true => Ok(()),
            false => self.list.process().await,
        }
    }

    pub fn is_just_text(&self) -> bool {
        self.is_just_text
    }

    fn get_template_end(text: String) -> Option<usize> {
        let mut pos: usize = 0;
        let mut curly_braces_open: usize = 2;
        let tv = text.as_bytes();
        while pos < tv.len() && curly_braces_open > 0 {
            match tv[pos] as char {
                '{' => curly_braces_open += 1,
                '}' => curly_braces_open -= 1,
                _ => {}
            }
            pos += 1;
        }
        if curly_braces_open == 0 {
            Some(pos)
        } else {
            None
        }
    }
}
