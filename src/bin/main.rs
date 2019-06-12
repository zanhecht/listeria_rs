extern crate config;
extern crate mediawiki;
//#[macro_use]
extern crate serde_json;

use config::{Config, File};
use roxmltree;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Template {
    pub title: String,
    pub params: HashMap<String, String>,
}

impl Template {
    pub fn new_from_xml(node: &roxmltree::Node) -> Option<Self> {
        let mut title: Option<String> = None;

        let mut parts: HashMap<String, String> = HashMap::new();
        for n in node.children().filter(|n| n.is_element()) {
            if n.tag_name().name() == "title" {
                n.children().for_each(|c| {
                    let t = c.text().unwrap_or("").replace("_", " ");
                    let t = t.trim();
                    title = Some(t.to_string());
                });
            } else if n.tag_name().name() == "part" {
                let mut k: Option<String> = None;
                let mut v: Option<String> = None;
                n.children().for_each(|c| {
                    let tag = c.tag_name().name();
                    match tag {
                        "name" => {
                            let txt: Vec<String> = c
                                .children()
                                .map(|c| c.text().unwrap_or("").trim().to_string())
                                .collect();
                            let txt = txt.join("");
                            if txt.is_empty() {
                                match c.attribute("index") {
                                    Some(i) => k = Some(i.to_string()),
                                    None => {}
                                }
                            } else {
                                k = Some(txt);
                            }
                        }
                        "value" => {
                            let txt: Vec<String> = c
                                .children()
                                .map(|c| c.text().unwrap_or("").trim().to_string())
                                .collect();
                            v = Some(txt.join(""));
                        }
                        _ => {}
                    }
                });

                /*
                let mut children = n.children();
                let k: Vec<String> = match children.next() {
                    Some(x) => match x.tag_name().name() {
                        "name" => x
                            .children()
                            .map(|c| c.text().unwrap_or("").trim().to_string())
                            .collect(),
                        _ => return None,
                    },
                    None => return None,
                };

                match children.next() {
                    Some(x) => match x.tag_name().name() {
                        "equals" => {}
                        _ => return None,
                    },
                    None => return None,
                };

                let v: Vec<String> = match children.next() {
                    Some(x) => match x.tag_name().name() {
                        "value" => x
                            .children()
                            .map(|c| c.text().unwrap_or("").trim().to_string())
                            .collect(),
                        _ => return None,
                    },
                    None => return None,
                };
                parts.insert(k.join(""), v.join(""));
                */
                match (k, v) {
                    (Some(k), Some(v)) => {
                        parts.insert(k, v);
                    }
                    _ => {}
                }
            }
        }

        match title {
            Some(t) => Some(Self {
                title: t,
                params: parts,
            }),
            None => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListeriaPage {
    mw_api: mediawiki::api::Api,
    page: String,
    template_title_start: String,
    template: Option<Template>,
}

impl ListeriaPage {
    pub fn new(mw_api: &mediawiki::api::Api, page: String) -> Option<Self> {
        let mut ret = Self {
            mw_api: mw_api.clone(),
            page: page,
            template_title_start: "Wikidata list".to_string(),
            template: None,
        };
        ret.load_page().ok();
        Some(ret)
    }

    pub fn load_page(self: &mut Self) -> Result<(), String> {
        let params: HashMap<String, String> = vec![
            ("action", "parse"),
            ("prop", "parsetree"),
            ("page", self.page.as_str()),
        ]
        .iter()
        .map(|x| (x.0.to_string(), x.1.to_string()))
        .collect();

        let result = self
            .mw_api
            .get_query_api_json(&params)
            .expect("Loading page failed");
        let doc = match result["parse"]["parsetree"]["*"].as_str() {
            Some(text) => roxmltree::Document::parse(&text).unwrap(),
            None => return Err(format!("No parse tree for {}", &self.page)),
        };
        doc.root()
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "template")
            .for_each(|node| {
                if self.template.is_some() {
                    return;
                }
                match Template::new_from_xml(&node) {
                    Some(t) => {
                        if t.title == self.template_title_start {
                            self.template = Some(t);
                        }
                    }
                    None => {}
                }
            });
        match &self.template {
            Some(_) => Ok(()),
            None => Err(format!(
                "No template '{}' found",
                &self.template_title_start
            )),
        }
    }
}

fn main() {
    let ini_file = "bot.ini";
    let mut settings = Config::default();
    settings
        .merge(File::with_name(ini_file))
        .expect(format!("Replica file '{}' can't be opened", ini_file).as_str());
    let user = settings.get_str("user.user").expect("No user name");
    let pass = settings.get_str("user.pass").expect("No user pass");

    let mut mw_api = mediawiki::api::Api::new("https://de.wikipedia.org/w/api.php")
        .expect("Could not connect to MW API");
    mw_api.login(user, pass).expect("Could not log in");

    //println!("{:?}", mw_api.get_site_info());
    let _page = ListeriaPage::new(&mw_api, "Benutzer:Magnus_Manske/listeria_test2".into());
}
