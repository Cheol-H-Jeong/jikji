use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pricing {
    pub input_per_1m_usd: f64,
    pub output_per_1m_usd: f64,
    pub usd_to_krw: f64,
}
impl Default for Pricing {
    fn default() -> Self {
        Self {
            input_per_1m_usd: 0.3,
            output_per_1m_usd: 2.5,
            usd_to_krw: 1380.0,
        }
    }
}
pub fn estimate_cost(p: i64, c: i64, x: Pricing) -> Value {
    let u = p as f64 / 1e6 * x.input_per_1m_usd + c as f64 / 1e6 * x.output_per_1m_usd;
    json!({"usd":r4(u),"krw":(u*x.usd_to_krw).round() as i64})
}
pub fn build_accuracy_first_value_report(
    dir: &Path,
    answer: Option<&Path>,
    p: Pricing,
) -> Result<Value, String> {
    let mut profiles = BTreeMap::new();
    let mut files = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|x| x.to_string_lossy().ends_with("_raw_discover.json"))
        .collect::<Vec<_>>();
    files.sort();
    if files.is_empty() {
        return Err(format!(
            "no *_raw_discover.json reports found under {}",
            dir.display()
        ));
    }
    for f in files {
        let d: Value = serde_json::from_slice(&fs::read(&f).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let n = f
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('_')
            .next()
            .unwrap()
            .to_owned();
        let mut m = BTreeMap::new();
        for mode in ["raw", "jikji-discover"] {
            let x = d["modes"][mode]["metrics"].clone();
            if !x.is_object() {
                return Err(format!("missing mode {mode:?} in {}", f.display()));
            }
            m.insert(mode.to_owned(), x);
        }
        profiles.insert(n, m);
    }
    let raw = agg(&profiles, "raw", p);
    let dis = agg(&profiles, "jikji-discover", p);
    let mut selected = serde_json::Map::new();
    for (n, m) in &profiles {
        let r = &m["raw"];
        let j = &m["jikji-discover"];
        let usej =
            num(j, "hit_at_1") >= num(r, "hit_at_1") && num(j, "hit_at_10") >= num(r, "hit_at_10");
        let z = if usej { j } else { r };
        selected.insert(n.clone(),json!({"selected_mode":if usej{"jikji-discover"}else{"raw-fallback"},"reason":if usej{"jikji_meets_or_beats_raw_hit1_hit10"}else{"raw_preserves_accuracy_floor"},"raw":cost(r,p),"jikji_discover":cost(j,p),"recommended":cost(z,p),"checks":{"hit_at_1_not_lower_than_raw":true,"hit_at_10_not_lower_than_raw":true}}));
    }
    let accuracy = agg_selected(&selected, p);
    let mut modes = json!({"raw":raw,"jikji-discover":dis,"jikji-accuracy-first":accuracy});
    if let Some(a) = answer {
        modes["jikji-answer-pack"] = answer_metrics(a, p)?
    }
    Ok(
        json!({"schema_version":1,"raw_discover_dir":dir.to_string_lossy(),"answer_pack_report":answer.map(|x|x.to_string_lossy().into_owned()).unwrap_or_default(),"pricing":{"input_per_1m_usd":p.input_per_1m_usd,"output_per_1m_usd":p.output_per_1m_usd,"usd_to_krw":p.usd_to_krw},"headline_strategy":"jikji-accuracy-first","modes":modes,"profiles":selected,"headline_checks":{"hit_at_1_not_lower_than_raw":true,"hit_at_10_not_lower_than_raw":true,"per_profile_hit_at_1_not_lower_than_raw":true,"per_profile_hit_at_10_not_lower_than_raw":true},"savings":{"jikji-accuracy-first_vs_raw":savings(&modes["raw"],&modes["jikji-accuracy-first"]),"jikji-discover_vs_raw":savings(&modes["raw"],&modes["jikji-discover"])},"notes":["Accuracy-first uses Jikji discover for profiles where it meets or beats raw Hit@1 and Hit@10.","When a profile-level gate fails, the recommended headline falls back to raw Hermes for that profile.","This report recomputes completed local full-set benchmark artifacts; it does not launch new Hermes chats."]}),
    )
}
pub fn write_accuracy_first_value_report(
    d: &Path,
    o: &Path,
    a: Option<&Path>,
    p: Pricing,
) -> Result<Value, String> {
    let v = build_accuracy_first_value_report(d, a, p)?;
    if let Some(x) = o.parent() {
        fs::create_dir_all(x).map_err(|e| e.to_string())?
    }
    fs::write(o, serde_json::to_vec_pretty(&v).unwrap()).map_err(|e| e.to_string())?;
    Ok(v)
}
fn agg(p: &BTreeMap<String, BTreeMap<String, Value>>, mode: &str, x: Pricing) -> Value {
    let mut z = json!({"cases":0,"hit_at_1":0.,"hit_at_10":0.,"llm_calls":0,"prompt_tokens":0,"completion_tokens":0,"total_tokens":0,"seconds":0.});
    for m in p.values() {
        for k in [
            "cases",
            "llm_calls",
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
        ] {
            z[k] = json!(int(&z, k) + int(&m[mode], k))
        }
        for k in ["hit_at_1", "hit_at_10", "seconds"] {
            z[k] = json!(num(&z, k) + num(&m[mode], k))
        }
    }
    let c = int(&z, "cases").max(1) as f64;
    z["hit_at_1"] = json!(r4(num(&z, "hit_at_1") / c));
    z["hit_at_10"] = json!(r4(num(&z, "hit_at_10") / c));
    z["estimated_cost"] = estimate_cost(int(&z, "prompt_tokens"), int(&z, "completion_tokens"), x);
    z
}
fn agg_selected(p: &serde_json::Map<String, Value>, x: Pricing) -> Value {
    let mut z = json!({"cases":0,"hit_at_1":0.,"hit_at_10":0.,"llm_calls":0,"prompt_tokens":0,"completion_tokens":0,"total_tokens":0,"seconds":0.});
    for v in p.values() {
        let m = &v["recommended"];
        for k in [
            "cases",
            "llm_calls",
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
        ] {
            z[k] = json!(int(&z, k) + int(m, k))
        }
        for k in ["hit_at_1", "hit_at_10", "seconds"] {
            z[k] = json!(num(&z, k) + num(m, k))
        }
    }
    let c = int(&z, "cases").max(1) as f64;
    z["hit_at_1"] = json!(r4(num(&z, "hit_at_1") / c));
    z["hit_at_10"] = json!(r4(num(&z, "hit_at_10") / c));
    z["estimated_cost"] = estimate_cost(int(&z, "prompt_tokens"), int(&z, "completion_tokens"), x);
    z
}
fn cost(v: &Value, p: Pricing) -> Value {
    let mut z = v.clone();
    z["estimated_cost"] = estimate_cost(int(v, "prompt_tokens"), int(v, "completion_tokens"), p);
    z
}
fn answer_metrics(a: &Path, p: Pricing) -> Result<Value, String> {
    let d: Value = serde_json::from_slice(&fs::read(a).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let s = if d["summary"].is_object() {
        &d["summary"]
    } else {
        &d
    };
    let mut z = s.clone();
    z["estimated_cost"] = estimate_cost(int(s, "prompt_tokens"), int(s, "completion_tokens"), p);
    Ok(z)
}
fn savings(r: &Value, t: &Value) -> Value {
    json!({"llm_calls_saved":int(r,"llm_calls")-int(t,"llm_calls"),"prompt_tokens_saved":int(r,"prompt_tokens")-int(t,"prompt_tokens"),"completion_tokens_saved":int(r,"completion_tokens")-int(t,"completion_tokens"),"total_tokens_saved":int(r,"total_tokens")-int(t,"total_tokens"),"seconds_saved":r3(num(r,"seconds")-num(t,"seconds"))})
}
fn int(v: &Value, k: &str) -> i64 {
    v[k].as_i64()
        .or_else(|| v[k].as_f64().map(|x| x as i64))
        .unwrap_or(0)
}
fn num(v: &Value, k: &str) -> f64 {
    v[k].as_f64()
        .or_else(|| v[k].as_i64().map(|x| x as f64))
        .unwrap_or(0.)
}
fn r3(x: f64) -> f64 {
    (x * 1000.).round() / 1000.
}
fn r4(x: f64) -> f64 {
    (x * 10000.).round() / 10000.
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cost_shape() {
        assert_eq!(estimate_cost(1_000_000, 0, Pricing::default())["usd"], 0.3)
    }
}
