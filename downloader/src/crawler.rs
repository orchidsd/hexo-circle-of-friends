use chrono::{FixedOffset, Utc};
use data_structures::metadata;
use feed_rs::parser;
use html_escape::decode_html_entities;
use reqwest_middleware::ClientWithMiddleware;
use std::{collections::HashMap, vec};
use tracing::{info, warn};
use url::{ParseError, Url};
// time zones
// +08:00
pub static BEIJING_OFFSET: Option<FixedOffset> = FixedOffset::east_opt(8 * 60 * 60);

pub async fn crawl_link_page<'a>(
    url: &str,
    theme: &str,
    css_rule: &serde_yaml::Value,
    client: &ClientWithMiddleware,
) -> Result<HashMap<&'a str, Vec<String>>, Box<dyn std::error::Error>> {
    if css_rule.is_mapping() {
        let theme_rule = match css_rule.get(theme) {
            Some(s) => s,
            None => panic!("`{theme}` field not found in css_rule"),
        };
        let html = client.get(url).send().await?.text().await?;
        let document = nipper::Document::from(&html);
        // 返回结果init
        let mut result: HashMap<&str, Vec<String>> = HashMap::new();
        for rule in ["author", "link", "avatar"] {
            let fields = theme_rule
                .get(rule)
                .ok_or(format!("`{theme}-{rule}` 字段缺失"))?;
            let fields = fields
                .as_sequence()
                .ok_or(format!("`{theme}-{rule}` 字段格式错误"))?;

            let mut res = vec![];
            for field in fields {
                let match_rule: &str = field
                    .get("selector")
                    .ok_or(format!("`{theme}-{rule}-selector` 字段缺失"))?
                    .as_str()
                    .ok_or(format!("`{theme}-{rule}-selector` 字段格式错误"))?;
                let attr = field
                    .get("attr")
                    .ok_or(format!("`{theme}-{rule}-attr` 字段缺失"))?
                    .as_str()
                    .ok_or(format!("`{theme}-{rule}-attr` 字段格式错误"))?;

                for elem in document.select(match_rule).iter() {
                    let parsed_field = match attr {
                        "text" => elem.text().to_string(),
                        _ => match elem.attr(attr).map(|r| r.to_string()) {
                            Some(v) => v,
                            None => continue,
                        },
                    };
                    // 解码 HTML 实体（如 &quot; &amp; 等）
                    let decoded_field = decode_html_entities(&parsed_field).to_string();
                    res.push(decoded_field);
                }
                // 当前规则获取到结果，则认为规则是有效的，短路后续规则
                if !res.is_empty() {
                    break;
                }
            }

            // info!("{:?}",html);

            result.insert(rule, res);
        }
        // DEBUG:
        // if result.len() < 4 {
        //     debug!(
        //         "页面：{}, 使用规则：{:?}, 解析结果：{:#?}",
        //         url, theme, result
        //     );
        // }
        Ok(result)
    } else {
        panic!("css_rule 格式错误");
    }
}

pub async fn crawl_post_page<'a>(
    url: &str,
    css_rules: &serde_yaml::Mapping,
    client: &ClientWithMiddleware,
) -> Result<HashMap<&'a str, Vec<String>>, Box<dyn std::error::Error>> {
    // let html = reqwest::get(url).await?.text().await?;
    // DEBUG:
    // debug!("{}", url);
    let html = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let document = nipper::Document::from(&html);
    // 返回结果init
    let mut result: HashMap<&str, Vec<String>> = HashMap::new();
    // 使用过的css规则
    let mut used_css_rules = vec![];
    'outer: for css_rule in css_rules {
        let use_theme = css_rule
            .0
            .as_str()
            .ok_or("无法解析字段，需要一个字符串".to_string())?;
        used_css_rules.push(use_theme.to_string());
        for current_field in ["title", "link", "created", "updated"] {
            let fields = css_rule
                .1
                .get(current_field)
                .ok_or(format!("`{use_theme}-{current_field}` 字段缺失"))?;
            let fields = fields
                .as_sequence()
                .ok_or(format!("`{use_theme}-{current_field}` 字段格式错误"))?;

            for field in fields {
                let match_rule: &str = field
                    .get("selector")
                    .ok_or(format!("`{use_theme}-{current_field}-selector` 字段缺失"))?
                    .as_str()
                    .ok_or(format!(
                        "`{use_theme}-{current_field}-selector` 字段格式错误"
                    ))?;
                let attr = field
                    .get("attr")
                    .ok_or(format!("`{use_theme}-{current_field}-attr` 字段缺失"))?
                    .as_str()
                    .ok_or(format!("`{use_theme}-{current_field}-attr` 字段格式错误"))?;

                let mut res = vec![];
                for elem in document.select(match_rule).iter() {
                    let parsed_field = match attr {
                        "text" => elem.text().to_string(),
                        _ => match elem.attr(attr).map(|r| r.to_string()) {
                            Some(v) => v,
                            None => continue,
                        },
                    };
                    // 解码 HTML 实体（如 &quot; &amp; 等）
                    let decoded_field = decode_html_entities(&parsed_field).to_string();
                    res.push(decoded_field);
                }
                if !res.is_empty() {
                    // DEBUG:
                    // debug!("{}-{}-{}-{}", use_theme, match_rule, attr, current_field);
                    if !result.contains_key(current_field) {
                        result.insert(current_field, res);
                    }
                    // 全部字段解析完毕
                    if result.len() == 4 {
                        break 'outer;
                    }
                } else {
                    // DEBUG:
                    // debug!(
                    //     "页面：{},字段：{},使用规则:{},解析结果：{:?}",
                    //     url, current_field, use_theme, res
                    // );
                };
            }
        }
    }
    // DEBUG:
    // if result.len() < 4 {
    //     debug!(
    //         "页面：{}, 已使用规则：{:?}, 解析结果：{:?}",
    //         url, used_css_rules, result
    //     );
    // }
    result.insert("rules", used_css_rules);
    Ok(result)
}

pub async fn crawl_post_page_feed(
    url: &str,
    base_url: &Url,
    client: &ClientWithMiddleware,
) -> Result<Vec<metadata::BasePosts>, Box<dyn std::error::Error>> {
    // DEBUG:
    // debug!("feed.....{}", url);
    let resp = client.get(url).send().await?.error_for_status()?;
    let content_length = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let html = resp.bytes().await;

    match html {
        Ok(html) => match parser::parse(html.as_ref()) {
            Ok(feed_from_xml) => {
                let posts = feed_entries_to_posts(feed_from_xml.entries, base_url);
                // 疑似截断：只解析出极少条目，但响应声明的体积很大 →
                // 服务器可能对请求限流截断（如 inkss.cn 对数据中心 IP 只返回前几篇）
                if posts.len() <= 3 && content_length.is_some_and(|n| n > 300 * 1024) {
                    info!(
                        "feed {} 疑似被截断(entries={}, content-length={:?})，尝试 Range 分块补齐",
                        url,
                        posts.len(),
                        content_length
                    );
                    if let Some(full) =
                        download_feed_with_range(client, url, content_length).await
                    {
                        if let Ok(feed) = parser::parse(full.as_ref()) {
                            let full_posts = feed_entries_to_posts(feed.entries, base_url);
                            if full_posts.len() > posts.len() {
                                return Ok(full_posts);
                            }
                        }
                    }
                }
                Ok(posts)
            }
            Err(_) => Ok(Vec::new()),
        },
        Err(e) => {
            // 全量下载失败（连接被截断/超时），尝试 Range 分块下载
            warn!("feed {} 全量下载失败: {}，尝试 Range 分块下载", url, e);
            if let Some(full) = download_feed_with_range(client, url, content_length).await {
                if let Ok(feed) = parser::parse(full.as_ref()) {
                    return Ok(feed_entries_to_posts(feed.entries, base_url));
                }
            }
            Err(e)
        }
    }
}

/// 将 feed 条目转换为 BasePosts
fn feed_entries_to_posts(
    entries: Vec<feed_rs::model::Entry>,
    base_url: &Url,
) -> Vec<metadata::BasePosts> {
    // 返回结果init
    let mut format_base_posts = vec![];
    for entry in entries {
        // 标题
        let title = entry.title.map_or(String::from("文章标题获取失败"), |t| {
            // 解码 HTML 实体
            decode_html_entities(&t.content).to_string()
        });
        // url链接
        let link = if !entry.links.is_empty() {
            entry.links[0].href.clone()
        } else {
            warn!("feed无法解析url链接");
            continue;
        };
        // 处理相对地址
        let link = match Url::parse(&link) {
            Ok(_) => link,
            Err(parse_error) => match parse_error {
                ParseError::RelativeUrlWithoutBase => match base_url.join(&link) {
                    Ok(completion_url) => completion_url.to_string(),
                    Err(e) => {
                        warn!("无法拼接相对地址：{},error:{}", link, e);
                        continue;
                    }
                },
                _ => {
                    warn!("无法处理地址：{}", link);
                    continue;
                }
            },
        };
        // 时间
        let published_time = entry
            .published
            .map(|t| tools::strptime_to_string_ymd(t.fixed_offset()));
        let updated_time = entry
            .updated
            .map(|t| tools::strptime_to_string_ymd(t.fixed_offset()));
        let fallback_time =
            tools::strptime_to_string_ymd(Utc::now().with_timezone(&BEIJING_OFFSET.unwrap()));

        let created = published_time
            .clone()
            .or(updated_time.clone())
            .unwrap_or(fallback_time.clone());
        let updated = updated_time.or(published_time).unwrap_or(fallback_time);
        let base_post =
            metadata::BasePosts::new(title, created, updated, link, "feed".to_string());
        format_base_posts.push(base_post);
    }
    format_base_posts
}

/// Range 分块下载：针对服务器对单次响应限流截断的场景
/// （如 inkss.cn 对数据中心 IP 只返回响应前部），逐段请求后拼接完整内容
async fn download_feed_with_range(
    client: &ClientWithMiddleware,
    url: &str,
    content_length: Option<u64>,
) -> Option<Vec<u8>> {
    const CHUNK_SIZE: u64 = 256 * 1024;
    const MAX_TOTAL: u64 = 5 * 1024 * 1024;

    // 总大小未知时用 HEAD 探测
    let total = match content_length {
        Some(n) if n > 0 => n,
        _ => {
            let head = client.head(url).send().await.ok()?;
            let len = head
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)?
                .to_str()
                .ok()?
                .parse::<u64>()
                .ok()?;
            if len == 0 {
                return None;
            }
            len
        }
    };
    if total > MAX_TOTAL {
        warn!("feed {} 体积过大({}B)，放弃分块下载", url, total);
        return None;
    }

    let mut buf = Vec::with_capacity(total as usize);
    let mut start: u64 = 0;
    while start < total {
        let end = (start + CHUNK_SIZE - 1).min(total - 1);
        let resp = client
            .get(url)
            .header("Range", format!("bytes={}-{}", start, end))
            .send()
            .await
            .ok()?;
        // 服务器不支持 Range，返回 200：第一段即为全量
        if resp.status() == reqwest::StatusCode::OK {
            return resp.bytes().await.ok().map(|b| b.to_vec());
        }
        let bytes = resp.error_for_status().ok()?.bytes().await.ok()?;
        buf.extend_from_slice(&bytes);
        start = end + 1;
    }
    Some(buf)
}
