use regex::Regex;
use scraper::{self, selectable::Selectable, ElementRef};
use std::cmp::min;
use std::rc::Weak;
use std::str;
use std::vec::Vec;

use tracing::{debug, info, warn};

pub mod defs;
use defs::*;

fn py_adopt_rx((rx, plcmnt): (&str, &str)) -> (String, String) {
    (rx.to_owned(), defs::prepare_rx_sub_for_replace(plcmnt))
}

fn select_contains<'a, 'b>(
    transformed_data: &mut TransformedData,
    soup: &'b scraper::ElementRef,
    rule: &'a ParserTransfromRule,
    level: usize,
    settings: &TransformSettings,
) -> Result<bool, TransformError> {
    let Some((text, (left, right))) = rule.is_contains_selector() else {
        return Ok(false);
    };

    let selector = scraper::Selector::parse(&left).unwrap();
    let tags: Vec<scraper::ElementRef<'_>> = if left.is_empty() {
        soup.select(&selector).collect()
    } else {
        vec![soup.clone()]
    };

    for ele in tags {
        if ele.text().find(|s| *s == text).is_none() {
            continue;
        }
        if right.is_empty() {
            transform_html_single(
                transformed_data,
                &ele,
                &rule.with_empty_selector(),
                level,
                settings,
            )?;
            continue;
        }

        let selector = scraper::Selector::parse(&right).unwrap();
        let tags: Vec<scraper::ElementRef<'_>> = ele.select(&selector).collect();

        for ele in tags {
            transform_html_single(
                transformed_data,
                &ele,
                &rule.with_empty_selector(),
                level,
                settings,
            )?;
        }
    }

    Ok(true)
}

fn transform_html_single<'a, 'b>(
    transformed_data: &mut TransformedData,
    soup: &'b scraper::ElementRef,
    rule: &'a ParserTransfromRule,
    level: usize,
    settings: &TransformSettings,
) -> Result<(), TransformError> {
    let debmr = "| ".repeat(level);
    debug!(
        "{debmr} rule [{}, {}, {}, {}, {:?}]",
        rule.selector,
        &rule.mapping,
        &rule.grouping,
        rule.children.len(),
        soup.id(),
    );
    if level >= settings.max_depth_level {
        return Err(TransformError::RecursiveError { level });
    }

    let handle_regex = |rx: &Vec<String>, txt: &str| {
        let (left, right) = py_adopt_rx((&rx[0], &rx[1]));
        let regex = Regex::new(&left).unwrap();
        return regex.replace_all(txt, right).into_owned();
    };
    let handle_attr = |selected_soup: &scraper::ElementRef, attr_name: &str| {
        let attr = selected_soup.attr(attr_name);
        return String::from(attr.unwrap_or(""));
    };

    let transformed_data_out: &mut TransformedData = {
        let (group, obj) = (!rule.grouping.is_empty(), !rule.mapping.is_empty());

        if group {
            transformed_data.as_group_list_wrapper(&rule.grouping)
        } else if obj {
            transformed_data.as_map_wrapper()
        } else {
            transformed_data
        }
    };

    // grouping has handled
    let rule = rule.with_empty_grouping();

    debug!("{debmr} transformed_data_out is {transformed_data_out}");

    if select_contains(transformed_data_out, &soup, &rule, level, settings)? {
        return Ok(());
    }

    let selected_soup = if !rule.selector.is_empty() {
        let selector_str: &str = rule.selector.as_str();
        let sc_selector = scraper::Selector::parse(selector_str).unwrap();
        let tags: Vec<scraper::ElementRef<'_>> = soup.select(&sc_selector).collect();
        debug!("{debmr} selector_str [{selector_str}] tags count [{}]", tags.len());
        
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
            for (idx, &ele) in tags.iter().enumerate() {
                debug!("{debmr} push tag items, tag index [{idx} {:?}]", ele.id());
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
        debug!("{debmr} handle one tag as current soup for mapping and chidren, tag [{:?}]", tags[0].id());
        tags[0]
    } else {
        *soup
    };

    let _ = soup;

    if !rule.mapping.is_empty() {
        let attr_name = if !rule.attribute_name.is_empty() {
            rule.attribute_name.as_str()
        } else {
            "text"
        };
        let text = if attr_name == "text" {
            selected_soup.text().collect::<Vec<_>>().join(" ")
        } else {
            handle_attr(&selected_soup, attr_name)
        };

        let handled_text = if !rule.regex_sub_value.is_empty() {
            handle_regex(&rule.regex_sub_value, &text.trim())
        } else {
            text.trim().to_string()
        };

        debug!(
            "{debmr} push value {}",
            &handled_text[0..min(handled_text.len(), 10)]
        );
        transformed_data_out
            .push_value_path(&rule.mapping, TransformedData::Value(handled_text.into()));
    }

    if !rule.children.is_empty() {
        debug!("{debmr} handling of children");
        transform_html_multi(
            if rule.children.len() > 1 {
                transformed_data_out.as_map_wrapper()
            } else {
                transformed_data_out
            },
            &selected_soup,
            rule.children.as_slice(),
            level + 1,
            settings,
        )?;
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
pub fn transform_html_map<'a, 'b, 'c>(
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

#[inline]
pub fn transform_html<'a, 'b, 'c>(
    html: &'b str,
    rules: &[ParserTransfromRule],
    settings: &TransformSettings,
) -> Result<TransformedData, TransformError> {
    let mut data = TransformedData::create_dict();
    transform_html_inner(&mut data, html, rules, settings)?;
    Ok(data)
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
    use std::{error::Error, fs::File, io::Read, path::PathBuf, str::FromStr};

    use tracing::info;
    use tracing_subscriber::fmt::format::FmtSpan;

    use super::*;

    fn prepare_test_logs() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_span_events(FmtSpan::CLOSE)
            .with_target(false)
            .with_env_filter("transform_html_lib::transform_html")
            .try_init();
    }

    #[test]
    fn main_test() {
        prepare_test_logs();
        type rl = ParserTransfromRule;

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
                selector: String::from("li /* comments */ "),
                mapping: String::from("lis"),
                ..Default::default()
            },
            rl {
                selector: String::from("ul li/*sss*/.test "),
                mapping: String::from("place2"),
                ..Default::default()
            },
            rl {
                selector: "div".to_string(),
                grouping: "div_group".into(),
                children: vec![rl {
                    selector: "li".into(),

                    ..Default::default()
                }]
                .into(),
                ..Default::default()
            },
            rl::from_str(r#"{ "selector": ".test1", "mapping": "test_json" }"#).unwrap(),
        ];
        let data = transform_html_map(html, &rules, &TransformSettings::default()).expect("Err");
        info!("{data:#?}");
        assert_eq!(data["place"], TransformedData::from("Foo"));
        assert_eq!(data["place"], data["place2"]);
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
        match transform_html_single(
            &mut transformed_data,
            &doc.root_element(),
            &rules[0],
            1,
            &TransformSettings {
                max_depth_level: 2,
                ..Default::default()
            },
        ) {
            Ok(_) => panic!("error is missing"),
            Err(err) => {
                println!("{}", err);
                println!("{:?}", err);
            }
        };

        match transform_html_single(
            &mut transformed_data,
            &doc.root_element(),
            &rules[1],
            0,
            &TransformSettings::default(),
        ) {
            Ok(_) => panic!("error is missing"),
            Err(err) => {
                println!("{}", err);
                println!("{:?}", err);
            }
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

    #[test]
    fn regex_test() -> Result<(), Box<dyn Error>> {
        prepare_test_logs();

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

        let re = Regex::new(r"(\d+)").unwrap();
        let res = re.replace_all("123", r"page-$1").into_owned();
        info!("res = [{res}]");

        // for fix shortage of rust-scraper
        let source_str = r#"li.property-facts__item:-soup-contains( "Property type:")/*("")*/ .property-facts__value"#;
        let re = Regex::new(r#":-soup-contains\(\s+?".*?"\s+?\)"#).unwrap();
        let res = re
            .replace_all(
                &source_str,
                format!("{BS_CONTAINS_MARKER}/*$1*/{BS_CONTAINS_MARKER}"),
            )
            .into_owned();
        info!("res = [{res}]");

        Ok(())
    }

    #[test]
    fn selector_contains_test() {
        prepare_test_logs();
        type rl = ParserTransfromRule;

        let html = r#"
        <html>
         <head></head>
         <body>
           <div>Here is <span>some text</span>.</div>
            <div>Here is some more text.</div>
           
         </body>
        </html>
        "#;
        let rules = [
            rl {
                selector: String::from("div:-soup-contains(\"some text\") span"),
                mapping: String::from("place"),
                ..Default::default()
            },
            rl {
                selector: String::from("div:-soup-contains(\"some text\") "),
                mapping: String::from("place2"),
                ..Default::default()
            },
        ];
        let data = transform_html_map(html, &rules, &TransformSettings::default()).expect("Err");
        info!("data = [{data:#?}]")
    }

    #[test]
    fn attr_selector_test() -> Result<(), anyhow::Error> {
        prepare_test_logs();
        type rl = ParserTransfromRule;
        let property_finder_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/test/property-finder.frament.html");

        let mut html = String::new();
        File::open(property_finder_path)?.read_to_string(&mut html)?;

        let rules = [rl {
            selector: String::from(
                r#"ul[aria-label="Properties"][role="list"] li[role="listitem"] article"#,
            ),
            // mapping: "properties".into(),
            grouping: "properties".into(),
            children: vec![rl {
                selector: ":scope > a".into(),
                mapping: "content".into(),
                // grouping: "href".into(),
                // attribute_name: "href".into(),
                // grouping: "links".into(),
                children: vec![
                    rl {
                        mapping: "url".into(),
                        attribute_name: "href".into(),
                        ..Default::default()
                    },
                    rl {
                        mapping: "source_link".into(),
                        attribute_name: "href".into(),
                        ..Default::default()
                    },
                ]
                .into(),
                ..Default::default()
            }]
            .into(),
            ..Default::default()
        }];
        let data = transform_html(&html, &rules, &TransformSettings::default()).expect("Err");

        info!("data = [\n{}\n]", data.to_json_string());

        // let rules = [rl {
        //     selector: String::from(
        //         r#"ul[aria-label="Properties"][role="list"] li[role="listitem"] article"#,
        //     ),
        //     // mapping: String::from("properties"),
        //     grouping: "links".into(),
        //     children: vec![rl {
        //         selector: "a".into(),
        //         // grouping: "links".into(),
        //         children: vec![rl {
        //             mapping: "url".into(),
        //             attribute_name: "href".into(),
        //             ..Default::default()
        //         }]
        //         .into(),
        //         ..Default::default()
        //     }]
        //     .into(),
        //     ..Default::default()
        // }];
        // let data = transform_html(&html, &rules, &TransformSettings::default()).expect("Err");

        // info!("data = [{data:#?}]");
        Ok(())
    }
}
