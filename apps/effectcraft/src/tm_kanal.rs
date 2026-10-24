//! TM AutoClip kontrol kanalı kimlik doğrulaması — yamalı Craft'ların ORTAK dosyası (TM AutoClip yerel yaması, 11 Eki 2026).
//!
//! Bu dosya dört programda (FilmCraft, EffectCraft, LightCraft, SoundCraft) AYNIDIR: kaynağı TM AutoClip deposunda
//! `harici/craft-yamalar/ortak/kanal.rs`; her programın `apps/<ad>/src/tm_kanal.rs` dosyasına kopyalanır.
//! Sözleşme: `harici/craft-yamalar/YAMALAR.md` → "Kontrol kanalı kimliği" (.NET karşılığı `CraftKontrolIstemcisi`).
//!
//! - Kontrol sunucusu (`--control <port>`) YALNIZ `TMAC_BELIRTEC` ortam değişkeninde geçerli biçimli (64 küçük harf onaltılık)
//!   belirteç varken açılır. Yoksa port hiç dinlenmez (oturumsuz / elle `--control` ile açılış kanalsız sürer).
//! - Her bağlantının İLK isteği `{"id":n,"method":"auth","params":{"token":"<belirteç>"}}` olmalı. Eşleşirse
//!   `{"id":n,"ok":true,"result":{"authenticated":true}}` döner ve bağlantı normal sunulur; eşleşmezse ya da ilk istek başka bir
//!   şeyse tek hata satırı yazılır ve bağlantı KAPATILIR (hiçbir komut arayüze ulaşmaz). Karşılaştırma sabit sürede.
//! - Ek bağımlılık YOK (Cargo.lock değişmesin): yalnız std + serde_json.
//!
//! Bu bir caydırıcıdır, DRM değil: aynı makinedeki başka yerel süreçlerin oturum boyunca Craft'a komut göndermesini engeller.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::OnceLock;

use serde_json::{Value, json};

/// Belirteç (ortam değişkeni; komut satırında değil) — açılış kapısıyla aynı değişken.
pub const BELIRTEC_DEGISKENI: &str = "TMAC_BELIRTEC";
/// Kimlik doğrulama metodu (PhotoCraft'ın kendi kanalıyla aynı ad ve yanıt biçimi).
pub const DOGRULAMA_METODU: &str = "auth";
/// Doğrulamadan önce okunan satırın üst sınırı (bayt): doğrulanmamış eş büyük bellek ayırtamasın.
const ILK_SATIR_SINIRI: usize = 4096;
/// Doğrulamadan önce atlanan boş satır sınırı.
const BOS_SATIR_SINIRI: usize = 16;

static BELIRTEC: OnceLock<Option<String>> = OnceLock::new();

/// Belirteç biçimi: tam 64 küçük harf onaltılık.
pub fn bicim_dogru(belirtec: &str) -> bool {
    belirtec.len() == 64 && belirtec.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Süreç ortamındaki belirteç (ilk çağrıda okunur ve saklanır; biçimi bozuksa yok sayılır).
pub fn belirtec() -> Option<&'static str> {
    BELIRTEC.get_or_init(|| std::env::var(BELIRTEC_DEGISKENI).ok().filter(|b| bicim_dogru(b))).as_deref()
}

/// Kontrol sunucusu açılabilir mi: belirteç yoksa `false` (çağıran portu hiç dinlemez) ve sebebi stderr'e yazılır.
pub fn kanal_acilabilir(program: &str) -> bool {
    if belirtec().is_some() {
        return true;
    }
    eprintln!(
        "{program}: TM AutoClip yaması — kontrol kanalı yalnız TM AutoClip oturum belirteciyle açılır ({BELIRTEC_DEGISKENI} yok ya da bozuk); kanal açılmadı."
    );
    false
}

/// Sabit sürede eşitlik (uzunluk farkı hemen `false` — belirteç uzunluğu gizli değil, 64).
pub fn sabit_surede_esit(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |fark, (x, y)| fark | (x ^ y)) == 0
}

/// Bir bağlantıda ilk isteğin kararı (saf işlev — birim testli).
#[derive(Debug, PartialEq)]
pub enum Karar {
    /// Belirteç doğru: bu yanıt yazılır, bağlantı sunulur.
    Kabul(Value),
    /// Ret: bu hata satırı yazılır, bağlantı kapatılır.
    Ret(Value),
}

/// İlk satırı (yeni satır hariç) beklenen belirteçle denetler.
pub fn ilk_istegi_denetle(satir: &[u8], beklenen: &str) -> Karar {
    let ret = |id: Value, sebep: &str| Karar::Ret(json!({"id": id, "ok": false, "error": format!("unauthorized: {sebep}; closing the connection")}));
    let Ok(metin) = std::str::from_utf8(satir) else {
        return ret(Value::Null, "invalid UTF-8");
    };
    let Ok(istek) = serde_json::from_str::<Value>(metin.trim()) else {
        return ret(Value::Null, "the first request must be a JSON auth request");
    };
    let id = istek.get("id").cloned().unwrap_or(Value::Null);
    if istek.get("method").and_then(Value::as_str) != Some(DOGRULAMA_METODU) {
        return ret(id, "the first request on a connection must be `auth`");
    }
    let verilen = istek.get("params").and_then(|p| p.get("token")).and_then(Value::as_str).unwrap_or("");
    if !sabit_surede_esit(verilen.as_bytes(), beklenen.as_bytes()) {
        return ret(id, "wrong token");
    }
    Karar::Kabul(json!({"id": id, "ok": true, "result": {"authenticated": true}}))
}

/// Bağlantının ilk isteğini doğrular. `Some(okuyucu)` → doğrulandı; kalan istekler bu okuyucudan (tamponda kalanlar kaybolmaz).
/// `None` → hata satırı yazıldı (yazılabildiyse), çağıran bağlantıyı kapatır.
pub fn dogrula<R: Read, W: Write>(read: R, out: &mut W) -> Option<BufReader<R>> {
    let mut okuyucu = BufReader::new(read);
    let beklenen = belirtec()?;
    let mut satir = Vec::new();
    let mut bos = 0;
    let karar = loop {
        satir.clear();
        let sinir = u64::try_from(ILK_SATIR_SINIRI).unwrap_or(u64::MAX).saturating_add(1);
        let n = (&mut okuyucu).take(sinir).read_until(b'\n', &mut satir).ok()?;
        if n == 0 {
            return None;
        }
        let tam = satir.last() == Some(&b'\n');
        while matches!(satir.last(), Some(b'\n' | b'\r')) {
            satir.pop();
        }
        if !tam && satir.len() > ILK_SATIR_SINIRI {
            break ilk_istegi_denetle(b"", beklenen);
        }
        if satir.iter().all(u8::is_ascii_whitespace) {
            bos += 1;
            if bos > BOS_SATIR_SINIRI {
                return None;
            }
            continue;
        }
        break ilk_istegi_denetle(&satir, beklenen);
    };
    match karar {
        Karar::Kabul(yanit) => {
            writeln!(out, "{yanit}").and_then(|()| out.flush()).ok()?;
            Some(okuyucu)
        }
        Karar::Ret(hata) => {
            let _ = writeln!(out, "{hata}").and_then(|()| out.flush());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn bicim() {
        assert!(bicim_dogru(B));
        assert!(!bicim_dogru(&B.to_uppercase()));
        assert!(!bicim_dogru(&B[1..]));
        assert!(!bicim_dogru(""));
    }

    #[test]
    fn dogru_belirtec_kabul() {
        let satir = format!("{{\"id\":7,\"method\":\"auth\",\"params\":{{\"token\":\"{B}\"}}}}");
        let Karar::Kabul(y) = ilk_istegi_denetle(satir.as_bytes(), B) else { panic!("kabul beklenirdi") };
        assert_eq!(y["id"], 7);
        assert_eq!(y["result"]["authenticated"], true);
    }

    #[test]
    fn yanlis_ya_da_eksik_belirtec_ret() {
        let yanlis = B.replace('0', "1");
        for satir in [
            format!("{{\"id\":1,\"method\":\"auth\",\"params\":{{\"token\":\"{yanlis}\"}}}}"),
            "{\"id\":1,\"method\":\"auth\",\"params\":{}}".to_string(),
            "{\"id\":1,\"method\":\"auth\"}".to_string(),
            "{\"id\":1,\"method\":\"ui.inspect\"}".to_string(),
            "{\"id\":1,\"method\":\"engine.execute\",\"params\":{\"command\":\"file.save\",\"token\":\"x\"}}".to_string(),
            "GET / HTTP/1.1".to_string(),
            "[1,2]".to_string(),
        ] {
            let Karar::Ret(h) = ilk_istegi_denetle(satir.as_bytes(), B) else { panic!("ret beklenirdi: {satir}") };
            assert_eq!(h["ok"], false);
            assert!(h["error"].as_str().unwrap().contains("unauthorized"));
        }
    }

    #[test]
    fn dogrulanan_baglantida_kalan_istekler_kaybolmaz() {
        // `dogrula` süreç ortamındaki belirteci okur; testte ortam yok → None (sunucu zaten açılmaz).
        if belirtec().is_none() {
            let mut out = Vec::new();
            assert!(dogrula(&b"{\"method\":\"auth\"}\n"[..], &mut out).is_none());
            return;
        }
        let girdi = format!("\n{{\"id\":1,\"method\":\"auth\",\"params\":{{\"token\":\"{}\"}}}}\n{{\"id\":2,\"method\":\"a\"}}\n", belirtec().unwrap());
        let mut out = Vec::new();
        let mut kalan = String::new();
        dogrula(girdi.as_bytes(), &mut out).expect("kabul").read_line(&mut kalan).unwrap();
        assert!(kalan.contains("\"id\":2"));
    }

    #[test]
    fn sabit_sure_esitlik() {
        assert!(sabit_surede_esit(b"abc", b"abc"));
        assert!(!sabit_surede_esit(b"abc", b"abd"));
        assert!(!sabit_surede_esit(b"abc", b"abcd"));
    }
}
