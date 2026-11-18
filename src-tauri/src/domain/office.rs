//! Visual dos bonecos do escritório da esteira: cores e peças que o desenho em pixel art sabe montar.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Estilos de cabelo, acessórios e roupas que o desenho trata (ver `headShape` e `torsoShape` em `scene.ts`).
pub const HAIR_STYLES: [&str; 7] = ["short", "curly", "long", "bun", "topknot", "beanie", "fade"];
pub const ACCESSORIES: [&str; 4] = ["glasses", "readers", "headset", "goggles"];
pub const OUTFITS: [&str; 7] = ["hoodie", "lanyard", "cardigan", "pearls", "coat", "scarf", "tie"];

/// Visual de um boneco. As sombras (`skin_shade`, `hair_light`, `shirt_shade`) são opcionais: sem elas, o desenho
/// deriva das cores base.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BotLook {
    pub skin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_shade: Option<String>,
    pub hair: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hair_light: Option<String>,
    pub shirt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shirt_shade: Option<String>,
    pub pants: String,
    pub hair_style: String,
    #[serde(default)]
    pub accessory: Option<String>,
    pub outfit: String,
}

/// Visual de cada papel (chave = papel da esteira). Papel fora do mapa fica com o boneco padrão.
pub type OfficeLooks = BTreeMap<String, BotLook>;

const FALLBACK: (&str, &str, &str, &str) = ("#d2a07a", "#3a2a20", "#6f7de0", "#4a5578");

/// Cor `#rrggbb` em minúsculas, ou `None` se não for uma.
fn hex(value: &str) -> Option<String> {
    let v = value.trim().to_ascii_lowercase();
    let v = if v.starts_with('#') { v } else { format!("#{v}") };
    (v.len() == 7 && v[1..].chars().all(|c| c.is_ascii_hexdigit())).then_some(v)
}

fn pick(value: &str, allowed: &[&str]) -> Option<String> {
    let v = value.trim().to_ascii_lowercase();
    allowed.contains(&v.as_str()).then_some(v)
}

impl BotLook {
    /// Corrige o que vier fora do formato: cor inválida vira a padrão do campo, peça desconhecida vira a primeira
    /// da lista e acessório desconhecido some.
    pub fn normalized(self) -> BotLook {
        BotLook {
            skin: hex(&self.skin).unwrap_or_else(|| FALLBACK.0.into()),
            skin_shade: self.skin_shade.as_deref().and_then(hex),
            hair: hex(&self.hair).unwrap_or_else(|| FALLBACK.1.into()),
            hair_light: self.hair_light.as_deref().and_then(hex),
            shirt: hex(&self.shirt).unwrap_or_else(|| FALLBACK.2.into()),
            shirt_shade: self.shirt_shade.as_deref().and_then(hex),
            pants: hex(&self.pants).unwrap_or_else(|| FALLBACK.3.into()),
            hair_style: pick(&self.hair_style, &HAIR_STYLES).unwrap_or_else(|| HAIR_STYLES[0].into()),
            accessory: self.accessory.as_deref().and_then(|a| pick(a, &ACCESSORIES)),
            outfit: pick(&self.outfit, &OUTFITS).unwrap_or_else(|| OUTFITS[0].into()),
        }
    }
}

/// Lê a resposta do modelo (JSON do visual, possivelmente cercado de texto) e corrige os campos fora do formato.
/// Devolve `None` se não houver JSON ou se faltar alguma das cores base.
pub fn parse_bot_look(text: &str) -> Option<BotLook> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(text.get(start..=end)?).ok()?;
    let s = |k: &str| v[k].as_str().map(str::to_owned);
    let look = BotLook {
        skin: s("skin")?,
        skin_shade: s("skin_shade"),
        hair: s("hair")?,
        hair_light: s("hair_light"),
        shirt: s("shirt")?,
        shirt_shade: s("shirt_shade"),
        pants: s("pants").unwrap_or_default(),
        hair_style: s("hair_style").unwrap_or_default(),
        accessory: s("accessory").filter(|a| !a.trim().is_empty() && a.trim() != "none"),
        outfit: s("outfit").unwrap_or_default(),
    };
    Some(look.normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_json_inside_text() {
        let text = r##"Aqui está: {"skin":"#E0AC7E","hair":"2b2320","shirt":"#49b5b0","pants":"#3d4560","hair_style":"curly","accessory":"glasses","outfit":"hoodie"} pronto"##;
        let look = parse_bot_look(text).unwrap();
        assert_eq!((look.skin.as_str(), look.hair.as_str()), ("#e0ac7e", "#2b2320"));
        assert_eq!((look.hair_style.as_str(), look.accessory.as_deref(), look.outfit.as_str()), ("curly", Some("glasses"), "hoodie"));
    }

    #[test]
    fn fixes_what_is_out_of_format() {
        let text = r##"{"skin":"bege","hair":"#123456","shirt":"#abcdef","pants":"#zzzzzz","hair_style":"moicano","accessory":"chapéu","outfit":"terno"}"##;
        let look = parse_bot_look(text).unwrap();
        assert_eq!(look.skin, FALLBACK.0);
        assert_eq!(look.pants, FALLBACK.3);
        assert_eq!((look.hair_style.as_str(), look.accessory, look.outfit.as_str()), ("short", None, "hoodie"));
    }

    #[test]
    fn needs_the_base_colors() {
        assert!(parse_bot_look("sem json").is_none());
        assert!(parse_bot_look(r##"{"skin":"#ffffff"}"##).is_none());
    }

    #[test]
    fn none_accessory_is_empty() {
        let look = parse_bot_look(r##"{"skin":"#fff","hair":"#000000","shirt":"#111111","accessory":"none"}"##).unwrap();
        assert_eq!(look.accessory, None);
    }
}
