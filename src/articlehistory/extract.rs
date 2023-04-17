use std::collections::HashMap;

use crate::articlehistory::ParameterType;

use super::{Info, Parameter, Result, Ty};
use parsoid::Template;

use tracing::warn;

/// first extract useful information from article history.
pub fn extract_info(article_history: &Template) -> Result<Option<Info>> {
    // extract all useable otd, dyk and itn params from the map. To not be disruptive, we record the last index of each parameter.
    let all_params = article_history.params();

    let interesting =
        |n: &str| n.starts_with("otd") || n.starts_with("itn") || n.starts_with("dyk");

    // the FIRST index of an otd/itn/dyk template. This is usually at the end of a
    // template, but to not be disruptive when removing and reinserting the parameters,
    // we keep the first index to reinsert later.
    let start_index = all_params.iter().position(|(name, _)| interesting(name));

    // split the params into otd/itn/dyk and those that are not.
    let (filtered, others): (Vec<_>, Vec<_>) = all_params
        .into_iter()
        .partition(|(name, _)| interesting(name));

    let mut params: HashMap<(Ty, usize), Parameter> = HashMap::new();

    for (name, value) in filtered {
        let param = &name[3..];
        let (pos, _) = param
            .char_indices()
            .find(|(_, c)| !c.is_ascii_digit())
            .unwrap();
        let (num, param) = param.split_at(pos);
        let num = if num.is_empty() { 1 } else { num.parse()? };
        let key = &name[0..3];
        let (key, mk): (_, fn() -> ParameterType) = match key {
            "itn" => (Ty::Itn, || ParameterType::Itn {
                date: None,
                link: None,
            }),
            "dyk" => (Ty::Dyk, || ParameterType::Dyk {
                date: None,
                entry: None,
                nom: None,
            }),
            "otd" => (Ty::Otd, || ParameterType::Otd {
                date: None,
                oldid: None,
                link: None,
            }),
            _ => unreachable!(),
        };

        let p = params.entry((key, num)).or_insert_with(|| Parameter {
            index: num,
            ty: mk(),
        });

        macro_rules! generate_match {
            ($(
                $name:ident {
                    $($field:ident: Option<String>),*$(,)?
                }
            ),*$(,)?) => {
                match &mut p.ty {
                    $(
                        ParameterType::$name { $($field,)* } => match param {
                            $(stringify!($field) => {
                                if $field.is_some() {
                                    warn!("parameter override: {}", stringify!($field));
                                    return Ok(None);
                                }

                                *$field = Some(value);
                            })*
                            x => {
                                warn!(?x, "unrecognized parameter");
                                return Ok(None);
                            }
                        }
                    )*
                    /* x => {
                        warn!(?x, "you are not supposed to be here");
                        return Ok(None);
                    } */
                }
            };
        }

        generate_match! {
            Itn {
                date: Option<String>,
                link: Option<String>,
            },
            Dyk {
                date: Option<String>,
                entry: Option<String>,
                nom: Option<String>,
            },
            Otd {
                date: Option<String>,
                oldid: Option<String>,
                link: Option<String>,
            },
        }
    }

    Ok(Some(Info {
        start_index,
        others,
        params,
    }))
}

pub type ExtractResultMulti = Result<Option<Vec<ParameterType>>>;
pub type ExtractResultSingle = Result<Option<ParameterType>>;

pub fn extract_dyk(t: &Template) -> ExtractResultSingle {
    let mut date = None;
    let mut year = None;
    let mut entry = None;
    let mut nom = None;
    for (name, val) in t.params() {
        match &*name {
            "1" => date = Some(val),
            "2" if val.chars().all(|c| c.is_ascii_digit()) => year = Some(val),
            "2" | "entry" => entry = Some(val),
            "nompage" => nom = Some(val),
            // ignored parameters
            "views" | "article" | "small" | "3" | "image" => {}
            _ => {
                warn!(?name, "unrecognized parameter");
                return Ok(None);
            }
        }
    }

    let date = date.map(|date| {
        if let Some(year) = year {
            format!("{date} {year}")
        } else {
            date
        }
    });

    Ok(Some(ParameterType::Dyk { date, entry, nom }))
}

pub fn extract_otd(t: &Template) -> ExtractResultMulti {
    #[derive(Default)]
    pub struct Otd {
        date: Option<String>,
        oldid: Option<String>,
    }
    let mut map: HashMap<u32, Otd> = HashMap::new();
    for (param, value) in t.params() {
        if let Some(num) = param.strip_prefix("date") {
            map.entry(num.parse()?).or_default().date = Some(value);
        } else if let Some(num) = param.strip_prefix("oldid") {
            map.entry(num.parse()?).or_default().oldid = Some(value);
        } else {
            warn!(?param, "unrecognized parameter");
            return Ok(None);
        }
    }
    Ok(Some(
        map.into_values()
            .map(|Otd { date, oldid }| ParameterType::Otd {
                date,
                oldid,
                link: None,
            })
            .collect(),
    ))
}

pub fn extract_itn(t: &Template) -> ExtractResultMulti {
    #[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Itn {
        date: Option<String>,
    }

    let mut date1 = None;
    let mut year1 = None;
    let mut map: HashMap<u32, Itn> = HashMap::new();
    for (param, value) in t.params() {
        if param == "1" {
            date1 = Some(value);
        } else if param == "2" {
            year1 = Some(value);
        } else if let Some(num) = param.strip_prefix("date") {
            map.entry(if num.is_empty() { 1 } else { num.parse()? })
                .or_default()
                .date = Some(value);
        } else if param.strip_prefix("oldid").is_some() || param.strip_prefix("alt").is_some() {
            // ignore oldid
        } else {
            warn!(?param, "unrecognized parameter");
            return Ok(None);
        }
    }

    if let Some(mut d) = date1 {
        if let Some(y) = year1 {
            d.push(' ');
            d.push_str(&y);
        }

        map.entry(1).or_default().date = Some(d);
    }

    Ok(Some(
        map.into_values()
            .map(|Itn { date }| ParameterType::Itn { date, link: None })
            .collect(),
    ))
}

pub fn extract_failed_ga(t: &Template) -> ExtractResultSingle {
    let mut date = None;
    let mut oldid = None;
    let mut page = None;
    let mut topic = None;
    for (param, value) in t.params() {
        match &*param {
            "1" | "date" => date = Some(value),
            "topic" => topic = Some(value),
            "page" => page = Some(value),
            "oldid" => oldid = Some(value),
            // Ignore
            "small" => {}
            _ => {
                warn!(?param, "unrecognized parameter");
                return Ok(None);
            }
        }
    }

    todo!()
}
