//! Bounded semantic CommonMark/source metadata reference exclusion. Opaque HTML
//! and unknown tagged metadata retain review rather than grant absent references.
use pulldown_cmark::{Event,Options,Parser,Tag,TagEnd};
fn normalized(source:&str)->Result<String,String>{let mut value=source.to_owned();for _ in 0..4{let next=urlencoding::decode(&value).map_err(|_|"Ambiguous source URL encoding")?.into_owned();if next==value{break}value=next;}Ok(value.chars().filter(|c|!matches!(c,'\t'|'\r'|'\n')).collect::<String>().to_ascii_lowercase())}
fn uri(source:&str,asset:&str,path:&str)->Result<(),String>{
 let value=normalized(source)?;let pin=format!("starlane-attachment://{}",asset.to_ascii_lowercase());
 if value.contains(&pin)||value.contains(&path.to_ascii_lowercase()){return Err("Selected attachment source remains protected".into())}Ok(())
}
fn markdown(source:&str,asset:&str,path:&str)->Result<(),String>{
 let options=Options::ENABLE_TABLES|Options::ENABLE_FOOTNOTES|Options::ENABLE_STRIKETHROUGH|Options::ENABLE_TASKLISTS|Options::ENABLE_GFM;
 let mut code=0usize;let mut text=String::new();let mut events=0usize;
 for event in Parser::new_ext(source,options){events+=1;if events>262144{return Err("Markdown event count exceeds proof bound".into())}match event{
  Event::Start(Tag::CodeBlock(_))=>code+=1,Event::End(TagEnd::CodeBlock)=>code=code.saturating_sub(1),
  Event::Start(Tag::Link{dest_url,..}|Tag::Image{dest_url,..})=>uri(&dest_url,asset,path)?,
  Event::Html(_)|Event::InlineHtml(_)=>return Err("Opaque HTML source requires retained review".into()),
  Event::Text(value)if code==0=>{text.push_str(&value);if text.len()>8*1024*1024{return Err("Markdown text exceeds proof bound".into())}},
  Event::End(TagEnd::Paragraph|TagEnd::Heading(_)|TagEnd::Item)|Event::SoftBreak|Event::HardBreak=>{uri(&text,asset,path)?;text.clear();},_=>()}}
 uri(&text,asset,path)
}
fn yaml(value:&serde_yaml::Value,asset:&str,path:&str,depth:usize,count:&mut usize,max_nodes:usize)->Result<(),String>{*count+=1;if depth>32||*count>max_nodes{return Err("Source metadata exceeds proof bound".into())}match value{serde_yaml::Value::String(s)=>uri(s,asset,path),serde_yaml::Value::Sequence(v)=>{for x in v{yaml(x,asset,path,depth+1,count,max_nodes)?}Ok(())},serde_yaml::Value::Mapping(v)=>{for(k,x)in v{yaml(k,asset,path,depth+1,count,max_nodes)?;yaml(x,asset,path,depth+1,count,max_nodes)?}Ok(())},serde_yaml::Value::Tagged(_)=>Err("Tagged source metadata requires review".into()),_=>Ok(())}}
fn json(value:&serde_json::Value,asset:&str,path:&str,depth:usize,count:&mut usize,max_nodes:usize)->Result<(),String>{*count+=1;if depth>32||*count>max_nodes{return Err("JSON source metadata exceeds proof bound".into())}match value{serde_json::Value::String(s)=>{if s.trim().eq_ignore_ascii_case(asset){return Err("Selected attachment structured ID remains protected".into())}source(s,asset,path,depth+1,count,max_nodes)},serde_json::Value::Array(v)=>{for x in v{json(x,asset,path,depth+1,count,max_nodes)?}Ok(())},serde_json::Value::Object(v)=>{for(k,x)in v{uri(k,asset,path)?;json(x,asset,path,depth+1,count,max_nodes)?}Ok(())},_=>Ok(())}}
fn source(raw:&str,asset:&str,path:&str,depth:usize,count:&mut usize,max_nodes:usize)->Result<(),String>{
 if depth>32||raw.len()>8*1024*1024{return Err("Source exceeds proof bound".into())}
 if let Ok(value)=serde_json::from_str::<serde_json::Value>(raw){return json(&value,asset,path,depth+1,count,max_nodes)}
 let normalized_lines=raw.replace("\r\n","\n");let raw=normalized_lines.strip_prefix('\u{feff}').unwrap_or(&normalized_lines);
 if let Some(rest)=raw.strip_prefix("---\n"){let(metadata,body)=rest.split_once("\n---\n").ok_or("Ambiguous source frontmatter")?;let value:serde_yaml::Value=serde_yaml::from_str(metadata).map_err(|_|"Source frontmatter parse refused")?;yaml(&value,asset,path,0,count,max_nodes)?;markdown(body,asset,path)}else{markdown(raw,asset,path)}
}
pub(crate) fn exclude(raw:&str,asset:&str,path:&str)->Result<(),String>{source(raw,asset,path,0,&mut 0,512)}
#[cfg(test)]mod tests{use super::*;
#[test]fn parsed_links_images_definitions_and_encoding_protect_exact_version(){let id="11111111-2222-3333-4444-555555555555";let path="spaces/one/attachments/page/body.bin";for text in [format!("[old](starlane-attachment://{id})"),format!("![old](starlane-attachment%3A%2F%2F{id})"),format!("[old][version]\n\n[version]: starlane-attachment://{id}"),format!("[old](starlane-attachment&#x3a;//{id})"),format!("[old](/api/v1/files/{path})"),format!("starlane-attachment%3A%2F%2F{id}")]{assert!(exclude(&text,id,path).unwrap_err().contains("Selected attachment"),"{text}")}}
#[test]fn ordinary_markdown_and_code_do_not_create_reference_grants(){let id="11111111-2222-3333-4444-555555555555";let path="spaces/one/attachments/page/body.bin";for text in ["# Heading\n\n**Ordinary** [other](https://example.test)".to_owned(),format!("```markdown\n[code](starlane-attachment://{id})\n```"),format!("`starlane-attachment://{id}`"),format!("Plain UUID example {id}"),"[unresolved][example]".into()]{assert!(exclude(&text,id,path).is_ok(),"{text}")}assert!(exclude("<img src='https://example.test/opaque'>",id,path).unwrap_err().contains("Opaque HTML"));}
#[test]fn structured_exact_asset_id_is_protected(){let id="11111111-2222-3333-4444-555555555555";assert!(exclude(&serde_json::json!({"attachment_id":id}).to_string(),id,"spaces/a/attachments/b/c").is_err());}
#[test]fn frontmatter_and_nested_json_decode_before_reference_checks(){let id="11111111-2222-3333-4444-555555555555";let raw=format!("---\ncover_image: \"starlane-attachment://{}\\u0035\"\n---\nBody",&id[..id.len()-1]);assert!(exclude(&raw,id,"spaces/a/attachments/b/c").unwrap_err().contains("Selected attachment"));let nested=serde_json::json!({"value":format!("{{\"cover\":\"starlane-attachment://{}\\u0035\"}}",&id[..id.len()-1])}).to_string();assert!(exclude(&nested,id,"spaces/a/attachments/b/c").unwrap_err().contains("Selected attachment"));assert!(exclude("---\ncover_image: \"https://example.test/plain.png\"\n---\n# Normal Markdown",id,"spaces/a/attachments/b/c").is_ok());}}

// An exact descriptor-authenticated immutable control may contain a complete
// historical SQL/role tree. The ordinary native metadata bound remains512.
pub(crate) fn exclude_owned_control(raw:&str,asset:&str,path:&str)->Result<(),String>{source(raw,asset,path,0,&mut 0,131072)}
pub(crate) fn exclude_application_projection(raw:&str,asset:&str,path:&str)->Result<(),String>{
 // Only a complete descriptor-authenticated control caller may use this
 // byte-proportional traversal allowance. Source keeps its8MiB/depth32 limits;
 // ordinary native metadata remains512 nodes and never uses this function.
 let budget=raw.len().max(131072).min(8*1024*1024);source(raw,asset,path,0,&mut 0,budget)
}
#[cfg(test)]mod certified_control_budget_tests{use super::*;
 const ASSET:&str="11111111-2222-3333-4444-555555555555";const PATH:&str="spaces/a/attachments/b/current.bin";
 #[test]fn complete_large_control_scales_to_bytes_ordinary_stays_small(){let raw=serde_json::to_string(&vec![0;140000]).unwrap();assert!(exclude_application_projection(&raw,ASSET,PATH).is_ok());assert!(exclude(&raw,ASSET,PATH).is_err());assert!(exclude_owned_control(&raw,ASSET,PATH).is_err());}
 #[test]fn late_control_id_and_encoded_uri_still_protect(){for protected in [ASSET.to_owned(),format!("[pin](starlane-attachment%3A%2F%2F{ASSET})"),serde_json::json!({"attachment_id":ASSET}).to_string(),format!("[path](/api/v1/files/{PATH})")]{let mut values=vec![serde_json::Value::Null;140000];values.push(serde_json::Value::String(protected));let raw=serde_json::to_string(&values).unwrap();assert!(exclude_application_projection(&raw,ASSET,PATH).is_err());}}
 #[test]fn source_depth_bytes_and_opaque_html_remain_bounded(){let mut value=serde_json::Value::Null;for _ in 0..40{value=serde_json::json!([value]);}assert!(exclude_application_projection(&value.to_string(),ASSET,PATH).is_err());assert!(exclude_application_projection(&" ".repeat(8*1024*1024+1),ASSET,PATH).is_err());assert!(exclude_application_projection("<img src='https://example.test/opaque'>",ASSET,PATH).is_err());}
}
