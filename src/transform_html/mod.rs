use regex::Regex;
use scraper;
use std::vec::Vec;
use std::str;

use tracing::debug;

pub mod defs;
use defs::*;

fn transform_html_single<'a, 'b>(
    transformed_data: &mut TransformedData,
    soup: &'b scraper::ElementRef,
    rule: &'a ParserTransfromRule,
    level: usize,
    settings: &TransformSettings,
) -> Result<(), TransformError > {
    if level >= settings.max_depth_level {
        return Err(TransformError::RecursiveError { level });
    }

    let handle_regex = |rx: &Vec<String>, txt: &str| {
        let (left, right) = (&rx[0], &rx[1]);
        let regex = Regex::new(&left).unwrap();
        return regex.replace_all(txt, right).into_owned();
    };
    let handle_attr = |selected_soup: &scraper::ElementRef, attr_name: &str| {
        let attr = selected_soup.attr(attr_name);
        return String::from(attr.unwrap_or(""));
    };

    let transformed_data_out: &mut TransformedData = if !rule.grouping.is_empty() {
        transformed_data.as_map_wrapper()
    } else {
        transformed_data
    };

    let mut selected_soup = soup;
    let mut tags: Vec<scraper::ElementRef<'_>> = Vec::new();

    if !rule.selector.is_empty() {
        let selector_str: &'a str = rule.selector.as_str();
        let sc_selector = scraper::Selector::parse(selector_str).unwrap();
        tags.extend(soup.select(&sc_selector));

        if tags.len() == 0 && rule.exception_on_not_found {
            return Err(TransformError::AtLeastOneTagNotFoundError {
                tag_name: rule.selector.clone(),
            });
        }
        if tags.len() == 0 {
            return Ok(());
        }
        if tags.len() > 1 {
            let nested_rule = rule.with_empty_selector();
            match transformed_data_out {
                TransformedData::List(lst) => {
                    for ele in tags {
                        debug!("push list one");
                        transform_html_single(
                            transformed_data_out,
                            &ele,
                            &nested_rule,
                            level + 1,
                            settings,
                        )?
                    }
                    return Ok(());
                }
                _ => {
                    let mut nested_data = TransformedData::List(TransformedData::create_data_vec());
                    for ele in tags {
                        debug!("push dict one");
                        transform_html_single(
                            &mut nested_data,
                            &ele,
                            &nested_rule,
                            level + 1,
                            settings,
                        )?
                    }
                    let key_name = if !nested_rule.grouping.is_empty() {
                        nested_rule.grouping
                    } else {
                        nested_rule.mapping
                    };

                    if !key_name.is_empty() {
                        debug!("attach list");
                        transformed_data_out.push_value_path(&key_name, nested_data);
                    }
                    return Ok(());
                }
            }
        }
        selected_soup = &tags[0];
    }

    let mappting = if !rule.mapping.is_empty() {
        rule.mapping.clone()
    } else if !rule.children.is_empty() {
        rule.grouping.clone()
    } else {
        Default::default()
    };

    if !mappting.is_empty() {
        let attr_name = if !rule.attribute_name.is_empty() {
            rule.attribute_name.as_str()
        } else {
            "text"
        };
        let text = if attr_name == "text" {
            selected_soup.text().collect::<Vec<_>>().join(" ")
        } else {
            handle_attr(selected_soup, attr_name)
        };

        let handled_text = if !rule.regex_sub_value.is_empty() {
            handle_regex(&rule.regex_sub_value, &text.trim())
        } else {
            text.trim().to_string()
        };

        transformed_data_out.push_value_path(
            &mappting,
            TransformedData::Value(String::from(handled_text)),
        );
    }

    if !rule.children.is_empty() {
        transform_html_multi(transformed_data_out, soup, rule.children.as_slice(), level, settings)?;
    }

    Ok(())
}

fn transform_html_multi<'a, 'b>(
    transoftmed_data: &mut TransformedData,
    soup: &'b scraper::ElementRef,
    rules: &[ParserTransfromRule],
    level: usize,
    settings: &TransformSettings,
) -> Result<(), TransformError> {
    for ele in rules {
        transform_html_single(transoftmed_data, soup, ele, level, settings)?
    }
    Ok(())
}

fn transform_html_inner<'a, 'b, 'c>(
    transformed_data: &'c mut TransformedData,
    html: &'b str,
    rules: &[ParserTransfromRule],
    settings: &TransformSettings,
) -> Result<(), TransformError> {
    let parsed = scraper::Html::parse_document(html);
    let soup = parsed.root_element();
    transform_html_multi(transformed_data, &soup, rules, 1, settings)
}

#[inline]
pub fn transform_html<'a, 'b, 'c>(
    html: &'b str,
    rules: &[ParserTransfromRule],
    settings: &TransformSettings,
) -> Result<DataMap, TransformError> {
    let mut data = TransformedData::Dict(TransformedData::create_data_map());
    transform_html_inner(&mut data, html, rules, settings)?;
    match data {
        TransformedData::Dict(d) => Ok(d),
        _ => panic!("transform_html {UNSUPPORTED_ENUM_TYPE}"),
    }
}

pub fn transform_html_list<'a, 'b, 'c>(
    transformed_data: DataVec,
    html: &'b str,
    rules: &[ParserTransfromRule],
    settings: &TransformSettings,
) -> Result<(), TransformError> {
    let mut data = TransformedData::List(transformed_data);
    transform_html_inner(&mut data, html, rules, settings)
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn main_test() {
        type rl = ParserTransfromRule;
        // tracing_subscriber::fmt().with_max_level(Level::DEBUG).init();
        // tracing_subscriber::fmt().with_env_filter("declarative_scraper::transform_html").init();

        let html = r#"
            <ul>
                <li class="test">Foo</li>
                <li>Bar</li>
                <li>Baz</li>
            </ul>
            <ul>
                <li class="test1">Foo1</li>
                <li>Bar1</li>
                <li>Baz1</li>
            </ul>
        "#;
        let rules = [
            rl {
                selector: String::from(".test"),
                mapping: String::from("place"),
                ..Default::default()
            },
            rl {
                selector: String::from("ul"),
                mapping: String::from("ul"),
                ..Default::default()
            },
            rl {
                selector: String::from("li"),
                mapping: String::from("lis"),
                ..Default::default()
            },
            rl::from_str(r#"{ "selector": ".test1", "mapping": "test_json" }"#).unwrap(),
        ];
        let data = transform_html(
            html,
            &rules,
            &TransformSettings::default()
        )
        .expect("Err");
        assert_eq!(data["place"], TransformedData::from("Foo"));
        assert_eq!(data["test_json"], "Foo1".into());
    }

    #[test]
    fn err_test() {
        type RL<'c> = ParserTransfromRule;
        
        let html = r#"
            <div>
            <div>
            <ul>
                <li class="test">Foo</li>
                <li>Bar</li>
                <li>Baz</li>
            </ul>
            <ul>
                <li class="test1">Foo1</li>
                <li>Bar1</li>
                <li>Baz1</li>
            </ul>
            </div>
            </div>
        "#;

        let rules = [
            RL {
                selector: String::from("li"),
                mapping: String::from("place"),
                ..Default::default()
            },
            RL {
                selector: "h1".to_string(),
                mapping: "h1_empty".into(),
                exception_on_not_found: true,
                ..Default::default()
            },
        ];

        let mut transformed_data = TransformedData::create_dict();
        let doc = scraper::Html::parse_document(html);
        match transform_html_single(&mut transformed_data, &doc.root_element(), &rules[0], 1, &TransformSettings { max_depth_level: 2 }) {
            Ok(_) => panic!("error is missing"),
            Err(err) => { 
                println!("{}", err);
                println!("{:?}", err);
            },
        };

        match transform_html_single(&mut transformed_data, &doc.root_element(), &rules[1], 0, &TransformSettings::default()) {
            Ok(_) => panic!("error is missing"),
            Err(err) => { 
                println!("{}", err);
                println!("{:?}", err);
            },
        };
        
    }

    #[test]
    fn test_err_kind() {
        use std::fs::File;
        use std::io::ErrorKind;

        let greeting_file_result = File::open("hello.txt");

        let greeting_file = match greeting_file_result {
            Ok(file) => file,
            Err(error) => match error.kind() {
                ErrorKind::NotFound => match File::create("target/hello.txt") {
                    Ok(fc) => fc,
                    Err(e) => panic!("Problem creating the file: {:?}", e),
                },
                other_error => {
                    panic!("Problem opening the file: {:?}", other_error);
                }

            },
        };
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod deps_tests {
    use std::error::Error;
    use tracing::info;

    use super::*;

    // see: https://iproyal.com/blog/web-scraping-with-rust-the-ultimate-guide/
    #[tokio::test]
    async fn parse_web_page_with_scraper() -> Result<(), Box<dyn Error>> {
        tracing_subscriber::fmt::init();

        let client = reqwest::Client::builder().build()?;
        let response = client
            .get("https://news.ycombinator.com/")
            .send()
            .await?
            .text()
            .await?;

        let document = scraper::Html::parse_document(&response);
        // document.root_element().select(selector)
        let title_selector = scraper::Selector::parse("span.titleline>a").unwrap();

        let titles = document.select(&title_selector).map(|x| x.inner_html());

        let titles_c = titles.collect::<Vec<String>>();
        info!("titles {titles_c:?}");

        let value = titles_c.len();
        assert_eq!(value, 30);

        Ok(())
    }

    #[test]
    fn regex_test() -> Result<(), Box<dyn Error>> {
        tracing_subscriber::fmt::init();

        let re = Regex::new(r"(?m)^([^:]+):([0-9]+):(.+)$").unwrap();
        let hay = "\
path/to/foo:54:Blue Harvest
path/to/bar:90:Something, Something, Something, Dark Side
path/to/baz:3:It's a Trap!
        ";
        info!("hay [{hay}]");
        let mut results = vec![];
        for (_, [path, lineno, line]) in re.captures_iter(hay).map(|c| c.extract()) {
            results.push((path, lineno.parse::<u64>()?, line));
        }
        assert_eq!(
            results,
            vec![
                ("path/to/foo", 54, "Blue Harvest"),
                (
                    "path/to/bar",
                    90,
                    "Something, Something, Something, Dark Side"
                ),
                ("path/to/baz", 3, "It's a Trap!"),
            ]
        );

        Ok(())
    }
}
