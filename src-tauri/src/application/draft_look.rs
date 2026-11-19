//! Sugere, com o próprio modelo, o visual de um boneco do escritório a partir de uma foto.

use super::{
    draft_agent::{one_shot, OneShot},
    ports::{AccountRepository, ChatEngine, Clock},
    AppError, AppResult,
};
use crate::domain::{
    chat::{Attachment, AttachmentKind},
    office::{parse_bot_look, BotLook, ACCESSORIES, BEARDS, HAIR_STYLES, OUTFITS},
};
use std::sync::Arc;

pub struct DraftLook {
    pub engine: Arc<dyn ChatEngine>,
    pub accounts: Arc<dyn AccountRepository>,
    pub clock: Arc<dyn Clock>,
    /// Pasta neutra em que o turno roda.
    pub chat_dir: String,
}

/// O formato da resposta e as peças que o desenho sabe montar.
fn system_prompt() -> String {
    format!(
        "Você transforma a foto de uma pessoa num boneco chibi pequeno (cabeça grande, desenho liso), visto em 3/4. \
         Olhe a pessoa principal da foto e responda apenas com um JSON, sem comentários, no formato \
         {{\"skin\": \"#rrggbb\", \"skin_shade\": \"#rrggbb\", \"hair\": \"#rrggbb\", \"hair_light\": \"#rrggbb\", \
         \"shirt\": \"#rrggbb\", \"shirt_shade\": \"#rrggbb\", \"pants\": \"#rrggbb\", \"hair_style\": \"...\", \
         \"accessory\": \"...\" ou null, \"outfit\": \"...\", \"beard\": \"...\", \"beard_color\": \"#rrggbb\" ou null}}.\n\
         - skin: tom de pele médio da pessoa; skin_shade: o mesmo tom uns 15% mais escuro.\n\
         - hair: cor do cabelo (ou da barba, se for careca); hair_light: um brilho um pouco mais claro.\n\
         - shirt: cor predominante da roupa de cima; shirt_shade: a mesma uns 25% mais escura; pants: cor da calça \
         (se não aparecer, um tom neutro escuro que combine).\n\
         - hair_style, um destes: {hair} (short = curto, curly = cacheado/volumoso, long = comprido, bun = coque \
         atrás, topknot = coque no alto, beanie = touca/gorro, fade = raspado nas laterais ou careca).\n\
         - accessory, um destes ou null: {acc} (glasses = óculos de armação escura, readers = óculos de armação \
         clara ou metálica, headset = fone de ouvido, goggles = óculos de proteção).\n\
         - outfit, o que mais lembrar a roupa: {out} (hoodie = moletom/camiseta, lanyard = crachá, cardigan = \
         casaco aberto/camisa social aberta, pearls = colar, coat = jaleco/blazer, scarf = cachecol, tie = gravata, \
         gremio = camisa do Grêmio, vneck = camiseta lisa de gola V).\n\
         - beard, um destes: {beard} (none = sem barba, stubble = barba curta/por fazer, full = barba cheia); \
         beard_color: a cor da barba se for bem diferente da do cabelo, senão null.\n\
         Cores vivas o bastante para ler bem num boneco pequeno; não use preto puro nem branco puro.",
        hair = HAIR_STYLES.join(", "),
        acc = ACCESSORIES.join(", "),
        out = OUTFITS.join(", "),
        beard = BEARDS.join(", "),
    )
}

impl DraftLook {
    pub async fn draft(&self, image: Attachment) -> AppResult<BotLook> {
        if image.kind != AttachmentKind::Image || image.data.is_empty() {
            return Err(AppError("Escolha uma imagem para gerar o boneco.".into()));
        }
        let text = one_shot(
            OneShot { engine: &*self.engine, accounts: &*self.accounts, clock: &*self.clock, chat_dir: &self.chat_dir },
            "Monte o boneco a partir desta foto.".into(),
            vec![image],
            system_prompt(),
            false,
        )
        .await?;
        parse_bot_look(&text).ok_or_else(|| AppError("O modelo não devolveu um visual válido. Tente de novo.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::account::sample;
    use crate::domain::chat::TurnEnd;

    fn photo() -> Attachment {
        Attachment { name: "foto.jpg".into(), kind: AttachmentKind::Image, media_type: "image/jpeg".into(), data: "AAAA".into() }
    }

    #[tokio::test]
    async fn sends_the_photo_and_reads_the_look() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.attachments.len(), 1);
            assert!(req.extra_system.as_deref().unwrap().contains("topknot"));
            assert!(req.tools.is_empty() && !req.interactive && !req.account_connectors);
            let text = r##"{"skin":"#c68c5f","hair":"#2b2320","shirt":"#e0894e","pants":"#55607d","hair_style":"short","accessory":null,"outfit":"hoodie"}"##;
            (vec![], TurnEnd::Done { session_id: None, text: text.into() })
        });
        let uc = DraftLook { engine: Arc::new(engine), accounts: world.store.clone(), clock: Arc::new(FixedClock(0)), chat_dir: "/tmp".into() };
        let look = uc.draft(photo()).await.unwrap();
        assert_eq!((look.skin.as_str(), look.accessory), ("#c68c5f", None));
        let empty = Attachment { data: String::new(), ..photo() };
        assert!(uc.draft(empty).await.is_err());
    }
}
