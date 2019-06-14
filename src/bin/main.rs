extern crate config;
extern crate mediawiki;
//#[macro_use]
extern crate serde_json;

use config::{Config, File};
use listeria::*;

fn main() {
    let ini_file = "listeria.ini";
    let page_title = "User:Magnus Manske/listeria test4"; //"Benutzer:Magnus_Manske/listeria_test2";
    let api_url = "https://en.wikipedia.org/w/api.php";

    let mut settings = Config::default();
    settings
        .merge(File::with_name(ini_file))
        .expect(format!("Replica file '{}' can't be opened", ini_file).as_str());
    let user = settings.get_str("user.user").expect("No user name");
    let pass = settings.get_str("user.pass").expect("No user pass");

    let mut mw_api = mediawiki::api::Api::new(api_url).expect("Could not connect to MW API");
    mw_api
        .login(user.to_owned(), pass.to_owned())
        .expect("Could not log in");

    let mut commons_api = mediawiki::api::Api::new("https://commons.wikimedia.org/w/api.php")
        .expect("Could not connect to Commons API");
    commons_api
        .login(user.to_owned(), pass.to_owned())
        .expect("Could not log in");

    let mut page = match ListeriaPage::new(&mw_api, page_title.into()) {
        Some(p) => p,
        None => panic!("Could not open/parse page '{}'", &page_title),
    };
    match page.run() {
        Ok(_) => {}
        Err(e) => panic!("{}", e),
    }
    let j = page.as_tabbed_data().unwrap();
    //println!("{}", ::serde_json::to_string_pretty(&j).unwrap());
    page.write_tabbed_data(j, &mut commons_api).unwrap();
}
