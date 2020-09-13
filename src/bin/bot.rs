extern crate config;
extern crate serde_json;

use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use listeria;
use crate::listeria::listeria_page::ListeriaPage;
use crate::listeria::configuration::Configuration;
use wikibase::mediawiki::api::Api;
use mysql_async::prelude::*;
use mysql_async::from_row;
use mysql_async as my;
use serde_json::Value;

// ssh magnus@tools-login.wmflabs.org -L 3308:tools-db:3306 -N

#[derive(Debug, Clone, Default)]
struct PageToProcess {
    id: u64,
    title: String,
    status: String,
    wiki: String,
}

#[derive(Debug, Clone)]
pub struct ListeriaBot {
    config: Arc<Configuration>,
    wiki_apis: HashMap<String,Arc<RwLock<Api>>>,
    pool: mysql_async::Pool,
    next_page_cache: Vec<PageToProcess>,
    site_matrix: Value,
}

impl ListeriaBot {
    pub async fn new(config_file: &str) -> Result<Self,String> {
        let config = Configuration::new_from_file(config_file).await?;

        let host = config.mysql("host").as_str().ok_or("No host in config")?.to_string();
        let schema = config.mysql("schema").as_str().ok_or("No schema in config")?.to_string();
        let port = config.mysql("port").as_u64().ok_or("No port in config")? as u16;
        let user = config.mysql("user").as_str().ok_or("No user in config")?.to_string();
        let password = config.mysql("password").as_str().ok_or("No password in config")?.to_string();

        let opts = my::OptsBuilder::default()
            .ip_or_hostname(host.to_owned())
            .db_name(Some(schema))
            .user(Some(user))
            .pass(Some(password))
            .tcp_port(port);

        // Load site matrix
        let api = config.get_default_wbapi()?;
        let params: HashMap<String, String> = vec![("action", "sitematrix")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let site_matrix = api.get_query_api_json(&params).await.expect("Can't run action=sitematrix on Wikidata API");
        let ret = Self {
            config: Arc::new(config),
            wiki_apis: HashMap::new(),
            pool: mysql_async::Pool::new(opts),
            next_page_cache: vec![],
            site_matrix,
        };

        Ok(ret)
    }

    fn get_url_for_wiki_from_site(&self, wiki: &str, site: &Value) -> Option<String> {
        self.get_value_from_site_matrix_entry(wiki, site, "dbname", "url")
    }

    fn get_value_from_site_matrix_entry(
        &self,
        value: &str,
        site: &Value,
        key_match: &str,
        key_return: &str,
    ) -> Option<String> {
        if site["closed"].as_str().is_some() {
            return None;
        }
        if site["private"].as_str().is_some() {
            return None;
        }
        match site[key_match].as_str() {
            Some(site_url) => {
                if value == site_url {
                    match site[key_return].as_str() {
                        Some(url) => Some(url.to_string()),
                        None => None,
                    }
                } else {
                    None
                }
            }
            None => None,
        }
    }

    fn get_server_url_for_wiki(&self, wiki: &str) -> Result<String, String> {
        match wiki.replace("_", "-").as_str() {
            "be-taraskwiki" | "be-x-oldwiki" => {
                return Ok("https://be-tarask.wikipedia.org".to_string())
            }
            _ => {}
        }
        self.site_matrix["sitematrix"]
            .as_object()
            .expect("AppState::get_server_url_for_wiki: sitematrix not an object")
            .iter()
            .filter_map(|(id, data)| match id.as_str() {
                "count" => None,
                "specials" => data
                    .as_array()
                    .expect("AppState::get_server_url_for_wiki: 'specials' is not an array")
                    .iter()
                    .filter_map(|site| self.get_url_for_wiki_from_site(wiki, site))
                    .next(),
                _other => match data["site"].as_array() {
                    Some(sites) => sites
                        .iter()
                        .filter_map(|site| self.get_url_for_wiki_from_site(wiki, site))
                        .next(),
                    None => None,
                },
            })
            .next()
            .ok_or(format!(
                "AppState::get_server_url_for_wiki: Cannot find server for wiki '{}'",
                &wiki
            ))
    }

    pub async fn process_next_page(&mut self) -> Result<(),String> {
        let page = self.get_next_page_to_process().await?;
        println!("Processing {} : {}",&page.wiki,&page.title);

        let mw_api = self.get_or_create_wiki_api(&page.wiki).await?;
        let mut listeria_page = match ListeriaPage::new(self.config.clone(), mw_api, page.title.to_owned()).await {
            Ok(p) => p,
            Err(e) => panic!("Could not open/parse page '{}': {}", &page.title,e),
        };
        match listeria_page.run().await {
            Ok(_) => {}
            Err(e) => panic!("{}", e),
        }
        //let renderer = RendererWikitext::new();
        //let old_wikitext = listeria_page.load_page_as("wikitext").await.expect("FAILED load page as wikitext");
        //let new_wikitext = renderer.get_new_wikitext(&old_wikitext,&listeria_page).unwrap().unwrap();
        //println!("{:?}",&new_wikitext);
        match listeria_page.update_source_page().await? {
            true => {println!("{} edited",&page.title);
            panic!("TEST");
            }
            false => println!("{} not edited",&page.title),
        }
        Ok(())
    }

    async fn get_or_create_wiki_api(&mut self, wiki: &str) -> Result<Arc<RwLock<Api>>,String> {
        match &self.wiki_apis.get(wiki) {
            Some(api) => { return Ok((*api).clone()); }
            None => {}
        }

        let api_url = format!("{}/w/api.php",self.get_server_url_for_wiki(wiki)?);
        let mut mw_api = wikibase::mediawiki::api::Api::new(&api_url)
            .await
            .expect("Could not connect to MW API");
        mw_api
            .login(self.config.wiki_user().to_owned(), self.config.wiki_password().to_owned())
            .await
            .expect("Could not log in");
        let mw_api = Arc::new(RwLock::new(mw_api));
        self.wiki_apis.insert(wiki.to_owned(),mw_api);
        
        self.wiki_apis.get(wiki).ok_or(format!("Wiki not found: {}",wiki)).map(|api|api.clone())
    }

    async fn get_next_page_to_process(&mut self) -> Result<PageToProcess,String> {
        if !self.next_page_cache.is_empty() {
            let page = self.next_page_cache.remove(0);
            return Ok(page);
        }

        let max_results : u64 = 100 ;
        
        let mut conn = self.pool.get_conn().await.expect("Can't connect to database");
        let sql = format!("SELECT pagestatus.id,pagestatus.page,pagestatus.status,wikis.name AS wiki FROM pagestatus,wikis WHERE pagestatus.wiki=wikis.id AND wikis.status='ACTIVE' AND pagestatus.status!='RUNNING' order by pagestatus.timestamp DESC LIMIT {}",max_results) ;
        self.next_page_cache = conn.exec_iter(
            sql.as_str(),
            ()
        ).await
        .map_err(|e|format!("PageList::run_batch_query: SQL query error[1]: {:?}",e))?
        .map_and_drop(|row| {
            let parts = from_row::<(u64,String,String,String)>(row);
            PageToProcess { id:parts.0, title:parts.1, status:parts.2, wiki:parts.3 }
        } )
        .await
        .map_err(|e|format!("PageList::run_batch_query: SQL query error[2]: {:?}",e))?;
        //println!("{:?}",&self.next_page_cache);
        conn.disconnect().await.map_err(|e|format!("{:?}",e))?;

        let page = self.next_page_cache.remove(0); // TODO check first
        Ok(page)
    }

    pub async fn destruct(&mut self) {
        //self.pool.disconnect().await.unwrap(); // TODO
    }

}

#[tokio::main]
async fn main() {
    let mut bot = ListeriaBot::new("config.json").await.unwrap();
    loop {
        match bot.process_next_page().await {
            Ok(()) => {}
            Err(e) => { println!("{}",&e); }
        }
    }
    /*
    let mut mw_api = wikibase::mediawiki::api::Api::new(api_url)
        .await
        .expect("Could not connect to MW API");
    let mw_api = Arc::new(RwLock::new(mw_api));
    */

}