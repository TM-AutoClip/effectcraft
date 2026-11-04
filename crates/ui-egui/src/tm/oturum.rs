//! TM AutoClip oturumu — yamalı Craft'ların ORTAK çekirdeği (TM AutoClip yerel yaması, 11 Eki 2026).
//!
//! Bu dosya beş programda (PhotoCraft, FilmCraft, LightCraft, EffectCraft, SoundCraft) AYNIDIR: kaynağı TM AutoClip deposunda
//! `harici/craft-yamalar/ortak/oturum.rs`; her programın `crates/ui-egui/src/tm/oturum.rs` dosyasına kopyalanır.
//! Sözleşme: `harici/craft-yamalar/YAMALAR.md` → "Oturum sözleşmesi" (.NET karşılığı `CraftOturumDosyasi.YamaDenetimi`).
//!
//! - `TMAC_OTURUM` = oturum.json yolu, `TMAC_BELIRTEC` = 64 küçük harf onaltılık belirteç (yalnız ortam değişkeninde).
//! - Kabul: `surum == 1`, `program` bu program, sha256(belirteç) == `belirtecSha256` (sabit sürede karşılaştırma),
//!   şimdi < `sonKullanma`, `kullanildiIsareti` dosyası YOK → işaret dosyası yazılır (tek kullanımlık).
//! - Ek bağımlılık YOK (Cargo.lock değişmesin): SHA-256 ve ISO-8601 okuma burada, yalnız serde_json kullanılır.
//! - Teslim (4 Kas 2026): çıktının özeti dosya parça parça okunarak çıkarılır (video GB'larca olabilir; eskiden bütün dosya
//!   belleğe alınıyordu) ve arka planda yazılabilir ([`teslim_arka_planda`]) — pencere özet sürerken donmaz.
//!
//! - Canlı uyumu (3 Kas 2026): yamalı exe canlıda bizim sunucumuzdan iner ve TM AutoClip menüsünden oturumsuz da açılır.
//!   Oturum ortam değişkeni (`TMAC_OTURUM`) HİÇ yoksa program serbest açılır (bizim öğeler gizli, kontrol kanalı kapalı);
//!   oturum VERİLDİYSE ve geçersizse (bozuk, süresi geçmiş, başka program, yanlış belirteç, kullanılmış) ret: [`kapi_karari`].
//!
//! Bu bir caydırıcıdır, DRM değil: resmî imzalı sürüm her zaman ayrıca indirilebilir.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::Value;

/// Oturum dosyasının yolu (ortam değişkeni).
pub const OTURUM_DEGISKENI: &str = "TMAC_OTURUM";
/// Belirteç (ortam değişkeni; komut satırında değil).
pub const BELIRTEC_DEGISKENI: &str = "TMAC_BELIRTEC";
/// Verilen oturum geçersizken gösterilen ana ileti (oturum hiç yoksa ileti yok, program serbest açılır).
pub const RET_ILETISI: &str = "Bu TM AutoClip oturumu geçersiz. Programı TM AutoClip'ten yeniden açın.";

/// Oturum kipi: `duz` = TM AutoClip menüsünden serbest açılış (bizim düğmeler gizli), `akis` = bizim akıştan (düğmeler görünür).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kip {
    Duz,
    Akis,
}

/// Marka linki (`craft-marka.json` → `linkler[]`); yalnız `https://` adresliler tutulur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub kimlik: String,
    pub etiket: String,
    pub adres: String,
}

/// Kabul edilmiş oturum.
#[derive(Clone, Debug)]
pub struct Oturum {
    pub kimlik: String,
    pub program: String,
    pub kip: Kip,
    pub is_klasoru: Option<PathBuf>,
    pub kaynaklar: Vec<PathBuf>,
    /// Beklenen çıktı: (tür: video | gorsel | ses, mutlak yol).
    pub cikti: Option<(String, PathBuf)>,
    pub teslim_dosyasi: Option<PathBuf>,
    /// Marka metinleri (`calisiyor`, `gonder`, `donus`); boşsa varsayılan kullanılır.
    pub metinler: Vec<(String, String)>,
    pub linkler: Vec<Link>,
}

impl Oturum {
    /// Bizim düğmeler görünür mü (akış kipi + çıktı + teslim dosyası tanımlı).
    pub fn akis_mi(&self) -> bool {
        self.kip == Kip::Akis && self.cikti.is_some() && self.teslim_dosyasi.is_some()
    }

    /// Marka metni (anahtar: `calisiyor` / `gonder` / `donus`), yoksa `varsayilan`.
    pub fn metin<'a>(&'a self, anahtar: &str, varsayilan: &'a str) -> &'a str {
        self.metinler.iter().find(|(k, v)| k == anahtar && !v.trim().is_empty()).map_or(varsayilan, |(_, v)| v.as_str())
    }
}

static OTURUM: OnceLock<Option<Oturum>> = OnceLock::new();

/// Açılışta kabul edilen oturum (kapı geçilmeden `None`).
pub fn simdiki() -> Option<&'static Oturum> {
    OTURUM.get().and_then(Option::as_ref)
}

/// Akış kipinde kabul edilmiş oturum (bizim düğmeler yalnız bu varken görünür).
pub fn akis() -> Option<&'static Oturum> {
    simdiki().filter(|o| o.akis_mi())
}

/// Açılış kapısının kararı: oturum ortam değişkeni yoksa serbest açılış (`Ok`, oturum yok); varsa [`ac`] ile denetlenir.
/// `Err` → çağıran [`RET_ILETISI`] + sebebi gösterip süreci kapatır.
pub fn kapi_karari(program: &str) -> Result<(), String> {
    if !oturum_istendi(std::env::var_os(OTURUM_DEGISKENI).as_deref()) {
        log::info!("TM AutoClip oturumu yok: serbest açılış ({program})");
        return Ok(());
    }
    ac(program).map(|_| ())
}

/// Ortamda oturum istendi mi (değişken var ve boş değil). Boş değer "yok" sayılır.
pub fn oturum_istendi(deger: Option<&std::ffi::OsStr>) -> bool {
    deger.is_some_and(|v| !v.is_empty())
}

/// Açılış kapısı: ortamdan oturumu okur, denetler, tek kullanımlık işareti yazar ve oturumu saklar.
/// `Err` → oturum yok ya da geçersiz (çağıran: [`kapi_karari`]).
pub fn ac(program: &str) -> Result<&'static Oturum, String> {
    if let Some(o) = simdiki() {
        return Ok(o);
    }
    let yol = std::env::var_os(OTURUM_DEGISKENI).filter(|v| !v.is_empty()).ok_or("TM AutoClip oturumu yok")?;
    let belirtec = std::env::var(BELIRTEC_DEGISKENI).map_err(|_| "TM AutoClip oturum belirteci yok".to_string())?;
    let metin = std::fs::read_to_string(&yol).map_err(|e| format!("oturum dosyası okunamadı: {e}"))?;
    let oturum = denetle(program, &metin, &belirtec, simdi_unix(), |p| Path::new(p).exists())?;
    // Tek kullanımlık: işaret dosyası YENİ oluşturulur; arada başkası yazdıysa (yarış) ret.
    let isaret = isaret_yolu(&metin)?;
    if !isaret.is_empty() {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&isaret)
            .map_err(|e| format!("belirteç zaten kullanıldı ya da işaret yazılamadı: {e}"))?;
    }
    log::info!("TM AutoClip oturumu kabul edildi ({}, kip {:?})", oturum.kimlik, oturum.kip);
    let _ = OTURUM.set(Some(oturum));
    simdiki().ok_or_else(|| "oturum saklanamadı".to_string())
}

fn isaret_yolu(metin: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(metin).map_err(|e| format!("oturum dosyası bozuk: {e}"))?;
    Ok(v.get("kullanildiIsareti").and_then(Value::as_str).unwrap_or("").to_string())
}

/// Saf denetim (dosya sistemi yalnız `dosya_var_mi` ile) — birim testli.
pub fn denetle(program: &str, metin: &str, belirtec: &str, simdi: i64, dosya_var_mi: impl Fn(&str) -> bool) -> Result<Oturum, String> {
    let v: Value = serde_json::from_str(metin).map_err(|e| format!("oturum dosyası bozuk: {e}"))?;
    let s = |ad: &str| v.get(ad).and_then(Value::as_str).unwrap_or("").to_string();
    if v.get("surum").and_then(Value::as_i64) != Some(1) {
        return Err("oturum biçimi tanınmıyor".into());
    }
    if !s("program").eq_ignore_ascii_case(program) {
        return Err("oturum başka program için".into());
    }
    if !belirtec_dogru(belirtec, &s("belirtecSha256")) {
        return Err("belirteç geçersiz".into());
    }
    let son = iso_unix(&s("sonKullanma")).ok_or("oturumun son kullanma anı okunamadı")?;
    if simdi >= son {
        return Err("belirtecin süresi geçti".into());
    }
    let isaret = s("kullanildiIsareti");
    if !isaret.is_empty() && dosya_var_mi(&isaret) {
        return Err("belirteç zaten kullanıldı".into());
    }
    let kip = match s("kip").as_str() {
        "akis" => Kip::Akis,
        _ => Kip::Duz,
    };
    let yol = |ad: &str| v.get(ad).and_then(Value::as_str).filter(|p| !p.is_empty()).map(PathBuf::from);
    let cikti = v.get("beklenenCikti").filter(|c| c.is_object()).and_then(|c| {
        let tur = c.get("tur").and_then(Value::as_str)?.to_string();
        let yol = c.get("yol").and_then(Value::as_str).filter(|p| !p.is_empty())?;
        Some((tur, PathBuf::from(yol)))
    });
    let marka = v.get("marka");
    let metinler = marka
        .and_then(|m| m.get("metin"))
        .and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
        .unwrap_or_default();
    let linkler = marka
        .and_then(|m| m.get("linkler"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    let al = |ad: &str| l.get(ad).and_then(Value::as_str).unwrap_or("").trim().to_string();
                    let (kimlik, etiket, adres) = (al("kimlik"), al("etiket"), al("adres"));
                    // Boş ya da https olmayan adres = düğme gizli (TM AutoClip de temizliyor; burada ikinci kez).
                    (adres.starts_with("https://") && !etiket.is_empty()).then_some(Link { kimlik, etiket, adres })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Oturum {
        kimlik: s("oturumKimligi"),
        program: s("program"),
        kip,
        is_klasoru: yol("isKlasoru"),
        kaynaklar: v
            .get("kaynaklar")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
            .unwrap_or_default(),
        cikti,
        teslim_dosyasi: yol("teslimDosyasi"),
        metinler,
        linkler,
    })
}

/// Teslim dosyasını yazar (önce `.tmp`, sonra yeniden adlandırma — TM AutoClip yarım dosya okumasın).
/// Biçim (YAMALAR.md "Teslim dosyası"): `{"surum":1,"oturumKimligi","program","durum":"tamam","cikti":{"tur","yol","boyut","sha256"},"zaman"}`.
pub fn teslim_yaz(oturum: &Oturum, cikti_yolu: &Path) -> Result<PathBuf, String> {
    let hedef = oturum.teslim_dosyasi.clone().ok_or("oturumda teslim dosyası yok")?;
    let tur = oturum.cikti.as_ref().map_or("", |(t, _)| t.as_str());
    let (boyut, ozet) = dosya_ozeti(cikti_yolu).map_err(|e| format!("çıktı okunamadı ({}): {e}", cikti_yolu.display()))?;
    if boyut == 0 {
        return Err(format!("çıktı boş: {}", cikti_yolu.display()));
    }
    let govde = serde_json::json!({
        "surum": 1,
        "oturumKimligi": oturum.kimlik,
        "program": oturum.program,
        "durum": "tamam",
        "cikti": {
            "tur": tur,
            "yol": cikti_yolu.to_string_lossy(),
            "boyut": boyut,
            "sha256": onaltilik(&ozet),
        },
        "zaman": simdi_unix(),
    });
    let gecici = hedef.with_extension("json.tmp");
    let metin = serde_json::to_string_pretty(&govde).map_err(|e| e.to_string())?;
    std::fs::write(&gecici, metin).map_err(|e| format!("teslim dosyası yazılamadı: {e}"))?;
    std::fs::rename(&gecici, &hedef).map_err(|e| format!("teslim dosyası yerine konamadı: {e}"))?;
    log::info!("TM AutoClip teslim dosyası yazıldı: {}", hedef.display());
    Ok(hedef)
}

/// [`teslim_yaz`]'ı arka planda çalıştırır (büyük videonun özeti saniyeler sürer; arayüz donmasın). Çağıran her karede
/// `is_finished` ile bakar, bitince `join` eder.
pub fn teslim_arka_planda(oturum: &'static Oturum, cikti_yolu: PathBuf) -> std::thread::JoinHandle<Result<PathBuf, String>> {
    std::thread::spawn(move || teslim_yaz(oturum, &cikti_yolu))
}

/// Dosyanın boyutu ve SHA-256'sı; dosya 1 MiB'lık parçalarla okunur (bellekte bütün dosya tutulmaz).
pub fn dosya_ozeti(yol: &Path) -> std::io::Result<(u64, [u8; 32])> {
    use std::io::Read;
    let mut dosya = std::fs::File::open(yol)?;
    let mut tampon = vec![0u8; 1 << 20];
    let mut ozet = Sha256::default();
    let mut boyut = 0u64;
    loop {
        let n = dosya.read(&mut tampon)?;
        if n == 0 {
            break;
        }
        ozet.ekle(&tampon[..n]);
        boyut += n as u64;
    }
    Ok((boyut, ozet.bitir()))
}

/// Şimdi (Unix saniyesi).
pub fn simdi_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Belirteç biçimi (64 küçük harf onaltılık) + özet eşleşmesi, sabit sürede.
pub fn belirtec_dogru(belirtec: &str, beklenen_ozet: &str) -> bool {
    let bicim = belirtec.len() == 64 && belirtec.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !bicim || beklenen_ozet.len() != 64 {
        return false;
    }
    let ozet = onaltilik(&sha256(belirtec.as_bytes()));
    let beklenen = beklenen_ozet.to_ascii_lowercase();
    ozet.bytes().zip(beklenen.bytes()).fold(0u8, |fark, (a, b)| fark | (a ^ b)) == 0
}

/// ISO-8601 (`2026-10-11T00:12:45.1234567+00:00`, `…Z`) → Unix saniyesi. Kesir atılır.
pub fn iso_unix(metin: &str) -> Option<i64> {
    let m = metin.trim();
    let sayi = |a: usize, b: usize| m.get(a..b).and_then(|s| s.parse::<i64>().ok());
    let (yil, ay, gun) = (sayi(0, 4)?, sayi(5, 7)?, sayi(8, 10)?);
    let (sa, dk, sn) = (sayi(11, 13)?, sayi(14, 16)?, sayi(17, 19)?);
    if m.as_bytes().get(4) != Some(&b'-') || m.as_bytes().get(10).is_none_or(|c| *c != b'T' && *c != b' ') {
        return None;
    }
    let mut kalan = &m[19..];
    if let Some(k) = kalan.strip_prefix('.') {
        kalan = k.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let fark = match kalan {
        "Z" | "z" | "" => 0,
        k if k.len() == 6 && (k.starts_with('+') || k.starts_with('-')) => {
            let isaret = if k.starts_with('-') { -1 } else { 1 };
            let (fs, fd) = (k.get(1..3)?.parse::<i64>().ok()?, k.get(4..6)?.parse::<i64>().ok()?);
            isaret * (fs * 3600 + fd * 60)
        }
        _ => return None,
    };
    if !(1..=12).contains(&ay) || !(1..=31).contains(&gun) || sa > 23 || dk > 59 || sn > 60 {
        return None;
    }
    Some(gunler_civil(yil, ay, gun) * 86_400 + sa * 3600 + dk * 60 + sn - fark)
}

/// 1970-01-01'den gün sayısı (Howard Hinnant, "days_from_civil").
fn gunler_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn onaltilik(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// SHA-256 (FIPS 180-4) — yeni bağımlılık eklememek için burada.
pub fn sha256(veri: &[u8]) -> [u8; 32] {
    let mut ozet = Sha256::default();
    ozet.ekle(veri);
    ozet.bitir()
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Akan SHA-256: veri parça parça eklenir ([`Sha256::ekle`]), sonunda özet alınır ([`Sha256::bitir`]).
#[derive(Clone)]
pub struct Sha256 {
    h: [u32; 8],
    blok: [u8; 64],
    dolu: usize,
    uzunluk: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self { h: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19], blok: [0; 64], dolu: 0, uzunluk: 0 }
    }
}

impl Sha256 {
    /// Veriyi ekler.
    pub fn ekle(&mut self, mut veri: &[u8]) {
        self.uzunluk = self.uzunluk.wrapping_add(veri.len() as u64);
        if self.dolu > 0 {
            let al = (64 - self.dolu).min(veri.len());
            self.blok[self.dolu..self.dolu + al].copy_from_slice(&veri[..al]);
            self.dolu += al;
            veri = &veri[al..];
            if self.dolu < 64 {
                return;
            }
            let blok = self.blok;
            sikistir(&mut self.h, &blok);
            self.dolu = 0;
        }
        let mut parcalar = veri.chunks_exact(64);
        for blok in &mut parcalar {
            sikistir(&mut self.h, blok);
        }
        let kalan = parcalar.remainder();
        self.blok[..kalan.len()].copy_from_slice(kalan);
        self.dolu = kalan.len();
    }

    /// Özeti verir.
    pub fn bitir(mut self) -> [u8; 32] {
        let bit_uzunluk = self.uzunluk.wrapping_mul(8);
        let mut son = [0u8; 128];
        son[..self.dolu].copy_from_slice(&self.blok[..self.dolu]);
        son[self.dolu] = 0x80;
        let toplam = if self.dolu < 56 { 64 } else { 128 };
        son[toplam - 8..toplam].copy_from_slice(&bit_uzunluk.to_be_bytes());
        for blok in son[..toplam].chunks_exact(64) {
            sikistir(&mut self.h, blok);
        }
        let mut cikti = [0u8; 32];
        for (i, x) in self.h.iter().enumerate() {
            cikti[i * 4..i * 4 + 4].copy_from_slice(&x.to_be_bytes());
        }
        cikti
    }
}

fn sikistir(h: &mut [u32; 8], blok: &[u8]) {
    let mut w = [0u32; 64];
    for (i, k) in blok.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([k[0], k[1], k[2], k[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(SHA256_K[i]).wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *x = x.wrapping_add(y);
    }
}

#[cfg(test)]
mod testler {
    use super::*;

    const BELIRTEC: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn ornek(program: &str, kip: &str, son: &str) -> String {
        let ozet = onaltilik(&sha256(BELIRTEC.as_bytes()));
        serde_json::json!({
            "surum": 1, "oturumKimligi": "o1", "program": program, "programSurumu": "0.6.0", "kip": kip,
            "belirtecSha256": ozet, "olusturma": "2026-10-11T00:00:00+00:00", "sonKullanma": son,
            "kullanildiIsareti": "/yok/isaret", "isKlasoru": "/is", "kaynaklar": ["/is/a.png"],
            "beklenenCikti": {"tur": "gorsel", "yol": "/is/cikti.png"}, "teslimDosyasi": "/is/teslim.json",
            "kontrolPortu": null,
            "marka": {"metin": {"donus": "Kaydet ve dön"}, "linkler": [
                {"kimlik": "discord", "etiket": "D", "adres": ""},
                {"kimlik": "site", "etiket": "S", "adres": "https://ornek.invalid"},
                {"kimlik": "kotu", "etiket": "K", "adres": "http://ornek.invalid"}]}
        })
        .to_string()
    }

    #[test]
    fn sha256_bilinen_degerler() {
        assert_eq!(onaltilik(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(onaltilik(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let uzun = vec![b'a'; 1000];
        assert_eq!(onaltilik(&sha256(&uzun)), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }

    #[test]
    fn akan_ozet_parcalardan_ayni() {
        let veri: Vec<u8> = (0..10_000u32).map(|i| (i * 31 % 251) as u8).collect();
        let tek = sha256(&veri);
        for boy in [1usize, 7, 55, 56, 63, 64, 65, 1000] {
            let mut s = Sha256::default();
            for parca in veri.chunks(boy) {
                s.ekle(parca);
            }
            assert_eq!(s.bitir(), tek, "parça boyu {boy}");
        }
    }

    #[test]
    fn dosya_ozeti_parca_parca_okur() {
        let yol = std::env::temp_dir().join(format!("tm-ozet-{}.bin", std::process::id()));
        let veri = vec![b'a'; (1 << 20) + 17];
        std::fs::write(&yol, &veri).unwrap();
        let (boyut, ozet) = dosya_ozeti(&yol).unwrap();
        let _ = std::fs::remove_file(&yol);
        assert_eq!(boyut, veri.len() as u64);
        assert_eq!(ozet, sha256(&veri));
    }

    #[test]
    fn iso_okuma() {
        assert_eq!(iso_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_unix("2026-10-11T00:00:00+00:00"), Some(1_791_676_800));
        assert_eq!(iso_unix("2026-10-11T03:00:00.1234567+03:00"), Some(1_791_676_800));
        assert_eq!(iso_unix("bozuk"), None);
    }

    #[test]
    fn gecerli_oturum_kabul() {
        let o = denetle("photocraft", &ornek("photocraft", "akis", "2026-10-11T00:02:00+00:00"), BELIRTEC, 1_791_676_800, |_| false).unwrap();
        assert!(o.akis_mi());
        assert_eq!(o.metin("donus", "x"), "Kaydet ve dön");
        assert_eq!(o.metin("gonder", "varsayılan"), "varsayılan");
        assert_eq!(o.linkler.len(), 1, "boş ve http adres gizli");
        assert_eq!(o.cikti.as_ref().unwrap().0, "gorsel");
    }

    #[test]
    fn oturumsuz_acilis_serbest() {
        assert!(!oturum_istendi(None), "değişken yok → serbest açılış");
        assert!(!oturum_istendi(Some(std::ffi::OsStr::new(""))), "boş değer → serbest açılış");
        assert!(oturum_istendi(Some(std::ffi::OsStr::new("/is/oturum.json"))), "oturum verildi → denetlenir");
    }

    #[test]
    fn ret_durumlari() {
        let m = ornek("photocraft", "duz", "2026-10-11T00:02:00+00:00");
        let simdi = 1_791_676_800;
        assert!(denetle("filmcraft", &m, BELIRTEC, simdi, |_| false).is_err(), "başka program");
        assert!(denetle("photocraft", &m, &BELIRTEC.replace('0', "1"), simdi, |_| false).is_err(), "yanlış belirteç");
        assert!(denetle("photocraft", &m, "KISA", simdi, |_| false).is_err(), "bozuk belirteç");
        assert!(denetle("photocraft", &m, BELIRTEC, simdi + 120, |_| false).is_err(), "süre geçti");
        assert!(denetle("photocraft", &m, BELIRTEC, simdi, |_| true).is_err(), "kullanılmış");
        assert!(denetle("photocraft", "{", BELIRTEC, simdi, |_| false).is_err(), "bozuk json");
        let duz = denetle("photocraft", &m, BELIRTEC, simdi, |_| false).unwrap();
        assert!(!duz.akis_mi(), "düz kipte bizim düğmeler gizli");
    }
}
