//! TM AutoClip yerel yaması — EffectCraft (4 Kas 2026). Upstream'e dokunuş: `lib.rs` → `pub mod tm;`, `app/frame.rs` → her
//! karede `tm::kare`, `menus/bar.rs` → Dosya menüsüne öğe, `menus/invoke.rs` → bizim komut, `apps/effectcraft/src/main.rs` → kapı.
//!
//! - Akış kipinde (TM AutoClip'in Render Kuyruğu'ndan açılış) Dosya ▸ "Render et ve TM AutoClip'e gönder": "TM AutoClip"
//!   kompozisyonunun oturumun beklenen çıktı yoluna yazan render kuyruğu öğesi (yoksa eklenir; kılavuz katmanlar render'a girmez)
//!   tek başına sıraya alınır, proje kaydedilir (TM AutoClip zaman çizelgesini kaydedilen projeden okur: kesimler, işaretler,
//!   altyazı), render arka planda yapılır; bitince `teslim.json` yazılır ve program kapanır.
//! - Kullanıcı Render Kuyruğu panelinden kendisi render ederse de: o öğe bitince proje kaydedilir ve teslim yazılır (program açık
//!   kalır; kapatınca TM AutoClip çıktıyı alır).
//! - "Render et ve gönder" yalnız bizim öğeyi render etsin diye kuyruktaki diğer işaretli öğelerin işaretini geçici kaldırır; render bitince
//!   (başarılı, hatalı ya da durdurulmuş) o öğelerin işareti geri konur (5 Kas 2026, geliştirici: "diğer öğelerin işaretleri geri gelsin").
//! - Düz kipte (oturumsuz ya da `kip = duz`) hiçbir şey görünmez, hiçbir şey yazılmaz.

pub mod oturum;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread::JoinHandle;

use effectcraft_engine::project::render_queue::RenderStatus;
use serde_json::{Value, json};

use crate::EffectcraftApp;

/// Bu programın oturumdaki adı.
pub const PROGRAM: &str = "effectcraft";
/// Bizim menü komutu (Dosya menüsü; kontrol kanalından da çağrılabilir).
pub const GONDER: &str = "tm.gonder";
/// TM AutoClip'in kurduğu kompozisyonun adı (.NET `EffectCraftScript.CompositionName`).
const KOMPOZISYON: &str = "TM AutoClip";

enum Durum {
    Bos,
    /// "Render et ve gönder" render'ı sürüyor.
    Render,
    /// Teslim arka planda yazılıyor (`kapat`: bitince program kapanır).
    Teslim(JoinHandle<Result<PathBuf, String>>, bool),
}

struct Hal {
    durum: Durum,
    /// Teslim edilen render'ın başlangıcı (aynı render iki kez teslim edilmesin).
    teslim_edilen: Option<u64>,
    /// Programın açıldığı an (Unix sn.): projede önceki bir oturumdan kalan "bitti" öğesi teslim edilmez.
    acilis: i64,
    /// "Render et ve gönder"in işaretini geçici kaldırdığı diğer öğeler (render bitince işaretleri geri konur).
    geri_isaret: Vec<u64>,
}

static HAL: Mutex<Option<Hal>> = Mutex::new(None);

fn hal<R>(f: impl FnOnce(&mut Hal) -> R) -> R {
    let mut kilit = HAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let h = kilit.get_or_insert_with(|| Hal { durum: Durum::Bos, teslim_edilen: None, acilis: oturum::simdi_unix(), geri_isaret: Vec::new() });
    f(h)
}

/// Dosya menüsünün sonuna bizim öğe (yalnız akış kipinde). Seçilince komut `clicked`'e konur, menü çağrısıyla aynı yoldan çalışır.
pub fn dosya_menusu(ui: &mut egui::Ui, clicked: &mut Option<(String, Value)>) {
    let Some(o) = oturum::akis() else { return };
    let mesgul = hal(|h| !matches!(h.durum, Durum::Bos));
    ui.separator();
    let etiket = o.metin("efektGonder", "Render et ve TM AutoClip'e gönder").to_string();
    if ui.add_enabled(!mesgul, egui::Button::new(etiket)).clicked() {
        *clicked = Some((GONDER.to_string(), json!({})));
        ui.close();
    }
}

/// Bizim komutlar (`None` = bizim değil).
pub fn calistir(app: &mut EffectcraftApp, id: &str) -> Option<Result<Value, String>> {
    (id == GONDER).then(|| baslat(app))
}

/// Beklenen çıktıya yazan render kuyruğu öğesinin sırası (çıktının klasörü ve adı; uzantı serbest).
fn hedef_oge(app: &EffectcraftApp, beklenen: &Path) -> Option<usize> {
    app.session.project.render_queue.iter().position(|it| ayni_cikti(Path::new(&it.output.output), beklenen))
}

fn ayni_cikti(yol: &Path, beklenen: &Path) -> bool {
    let kucuk = |p: Option<&std::ffi::OsStr>| p.map(|s| s.to_string_lossy().to_lowercase());
    let klasor = |p: &Path| p.parent().map(|d| d.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase());
    kucuk(yol.file_stem()).is_some() && kucuk(yol.file_stem()) == kucuk(beklenen.file_stem()) && klasor(yol) == klasor(beklenen)
}

/// "Render et ve TM AutoClip'e gönder".
fn baslat(app: &mut EffectcraftApp) -> Result<Value, String> {
    let o = oturum::akis().ok_or("TM AutoClip akış oturumu yok")?;
    if hal(|h| !matches!(h.durum, Durum::Bos)) {
        return Err("TM AutoClip'e gönderme zaten sürüyor".into());
    }
    if app.session.render_job.is_some() {
        return Err("Render sürüyor; bitince yeniden deneyin".into());
    }
    let (_, beklenen) = o.cikti.clone().ok_or("oturumda çıktı yolu yok")?;
    let calistir = |app: &mut EffectcraftApp, id: &str, p: Value| app.session.execute(id, p).map_err(|e| format!("{id}: {e}"));
    if hedef_oge(app, &beklenen).is_none() {
        // Öğe silinmişse yeniden: TM AutoClip'in kompozisyonu (yoksa etkin kompozisyon), ProRes 422 HQ, kılavuz katmanlar kapalı.
        let mut p = json!({"comp": KOMPOZISYON, "output": beklenen.to_string_lossy(), "format": "prores", "guideLayers": "allOff"});
        if calistir(app, "renderQueue.add", p.clone()).is_err() {
            p.as_object_mut().map(|m| m.remove("comp"));
            calistir(app, "renderQueue.add", p)?;
        }
        if let Some(i) = hedef_oge(app, &beklenen) {
            let id = app.session.project.render_queue[i].id;
            let _ = calistir(app, "renderQueue.setOutputModule", json!({"item": id, "proresProfile": "hq"}));
        }
    }
    let i = hedef_oge(app, &beklenen).ok_or("TM AutoClip render kuyruğu öğesi kurulamadı")?;
    let hedef_id = app.session.project.render_queue[i].id;
    // Yalnız bizim öğe render edilir (kuyruktaki diğer işaretli öğeler bu render'da beklesin; render bitince işaretleri geri konur).
    let digerleri: Vec<u64> = app.session.project.render_queue.iter().filter(|it| it.id != hedef_id && it.render).map(|it| it.id).collect();
    hal(|h| h.geri_isaret.clone_from(&digerleri));
    let basladi = (|| {
        for id in &digerleri {
            calistir(app, "renderQueue.setRender", json!({"item": id, "render": false}))?;
        }
        calistir(app, "renderQueue.setRender", json!({"item": hedef_id, "render": true}))?;
        // TM AutoClip kesimleri, işaretleri ve altyazıyı kaydedilen projeden okur: render'dan önce kaydet.
        calistir(app, "file.save", json!({}))?;
        calistir(app, "renderQueue.render", json!({"wait": false}))
    })();
    if let Err(e) = basladi {
        // Render başlamadı: kaldırılan işaretler hemen geri konur.
        geri_isaretle(app);
        return Err(e);
    }
    log::info!("TM AutoClip: render başladı ({})", beklenen.display());
    hal(|h| h.durum = Durum::Render);
    app.ui.status = "TM AutoClip için render ediliyor…".into();
    Ok(json!({"path": beklenen.to_string_lossy()}))
}

/// Her kare: bizim öğenin render'ı bittiyse proje kaydedilir ve teslim arka planda yazılır; teslim bitince (gönder komutuyla
/// başladıysa) program kapanır.
pub fn kare(app: &mut EffectcraftApp, ctx: &egui::Context) {
    let Some(o) = oturum::akis() else { return };
    let Some((_, beklenen)) = o.cikti.clone() else { return };
    let teslim = hal(|h| match std::mem::replace(&mut h.durum, Durum::Bos) {
        Durum::Teslim(is, kapat) if is.is_finished() => Some((is.join(), kapat)),
        diger => {
            h.durum = diger;
            None
        }
    });
    if let Some((sonuc, kapat)) = teslim {
        match sonuc {
            Ok(Ok(_)) if kapat => {
                app.ui.status = "TM AutoClip'e gönderildi".into();
                // Proje teslimden önce kaydedildi; kapatırken kaydet sorusu çıkmasın.
                app.dialog_state.unsaved.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Ok(Ok(_)) => app.ui.status = "TM AutoClip'e teslim edildi; EffectCraft'ı kapatınca Render Kuyruğu'na döner".into(),
            Ok(Err(e)) => {
                log::error!("TM AutoClip: {e}");
                app.ui.status = format!("TM AutoClip'e gönderilemedi: {e}");
            }
            Err(_) => {
                log::error!("TM AutoClip: teslim yazan iş çöktü");
                app.ui.status = "TM AutoClip'e gönderilemedi".into();
            }
        }
        return;
    }
    if hal(|h| matches!(h.durum, Durum::Teslim(..))) {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        return;
    }
    if app.session.render_job.is_some() {
        return;
    }
    let Some(i) = hedef_oge(app, &beklenen) else { return };
    let it = &app.session.project.render_queue[i];
    let gonderiyor = hal(|h| matches!(h.durum, Durum::Render));
    match &it.status {
        RenderStatus::Done => {
            let (baslangic, cikti) = (it.started, it.last_output.clone());
            let yeni = hal(|h| baslangic.is_some_and(|b| b as i64 >= h.acilis - 1) && h.teslim_edilen != baslangic);
            let Some(cikti) = cikti.filter(|_| yeni).map(PathBuf::from).filter(|p| p.is_file()) else {
                if gonderiyor {
                    geri_isaretle(app);
                    hal(|h| h.durum = Durum::Bos);
                    app.ui.status = "TM AutoClip'e gönderilemedi: render çıktısı bulunamadı".into();
                }
                return;
            };
            // Diğer öğelerin işaretleri geri (kaydedilen proje kullanıcının kuyruğunu eksiksiz taşısın).
            geri_isaretle(app);
            // Render'ın kuyruk durumu projeyi değiştirdi: TM AutoClip son hâli okusun diye yeniden kaydet.
            if let Err(e) = app.session.execute("file.save", json!({})) {
                log::warn!("TM AutoClip: proje teslimden önce kaydedilemedi: {e}");
            }
            log::info!("TM AutoClip: render bitti, teslim yazılıyor ({})", cikti.display());
            hal(|h| {
                h.teslim_edilen = baslangic;
                h.durum = Durum::Teslim(oturum::teslim_arka_planda(o, cikti), gonderiyor);
            });
            app.ui.status = "TM AutoClip için çıktı doğrulanıyor…".into();
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        RenderStatus::Failed(e) if gonderiyor => {
            let e = e.clone();
            geri_isaretle(app);
            hal(|h| h.durum = Durum::Bos);
            log::error!("TM AutoClip: render başarısız: {e}");
            app.ui.status = format!("TM AutoClip'e gönderilemedi: render başarısız ({e})");
        }
        RenderStatus::UserStopped if gonderiyor => {
            geri_isaretle(app);
            hal(|h| h.durum = Durum::Bos);
            app.ui.status = "TM AutoClip'e gönderme durduruldu".into();
        }
        _ => {}
    }
}

/// "Render et ve gönder"in işaretini kaldırdığı öğelerin işaretini geri koyar (kuyrukta hâlâ duran ve işareti kapalı olanlar). Konamayan
/// öğe günlüğe yazılır, diğerleri yine geri konur.
fn geri_isaretle(app: &mut EffectcraftApp) {
    let saklanan = hal(|h| std::mem::take(&mut h.geri_isaret));
    if saklanan.is_empty() {
        return;
    }
    let kuyruk: Vec<(u64, bool)> = app.session.project.render_queue.iter().map(|it| (it.id, it.render)).collect();
    for id in geri_isaretlenecekler(&kuyruk, &saklanan) {
        if let Err(e) = app.session.execute("renderQueue.setRender", json!({"item": id, "render": true})) {
            log::warn!("TM AutoClip: render kuyruğu öğesinin işareti geri konamadı ({id}): {e}");
        }
    }
}

/// Geri işaretlenecek öğeler: saklananlardan kuyrukta hâlâ duran ve işareti kapalı olanlar (silinen ya da kullanıcının yeniden işaretlediği atlanır).
fn geri_isaretlenecekler(kuyruk: &[(u64, bool)], saklanan: &[u64]) -> Vec<u64> {
    saklanan.iter().copied().filter(|id| kuyruk.iter().any(|(k, isaretli)| k == id && !isaretli)).collect()
}

#[cfg(test)]
mod tests {
    use super::{ayni_cikti, geri_isaretlenecekler};

    #[test]
    fn yalniz_kuyrukta_duran_ve_isaretsiz_ogeler_geri_isaretlenir() {
        let kuyruk = [(1, false), (2, true), (3, false), (9, true)];
        assert_eq!(geri_isaretlenecekler(&kuyruk, &[1, 2, 4]), vec![1], "2 zaten işaretli, 4 silinmiş");
        assert_eq!(geri_isaretlenecekler(&kuyruk, &[]), Vec::<u64>::new());
        assert_eq!(geri_isaretlenecekler(&kuyruk, &[3, 1]), vec![3, 1], "sıra korunur");
    }
    use std::path::Path;

    #[test]
    fn beklenen_cikti_klasor_ve_adla_eslesir() {
        let beklenen = Path::new("C:\\is\\cikti.mov");
        assert!(ayni_cikti(Path::new("C:/is/cikti.mov"), beklenen));
        assert!(ayni_cikti(Path::new("C:/IS/Cikti.mp4"), beklenen), "uzantı ve büyük harf serbest");
        assert!(!ayni_cikti(Path::new("C:/baska/cikti.mov"), beklenen));
        assert!(!ayni_cikti(Path::new("C:/is/baska.mov"), beklenen));
        assert!(!ayni_cikti(Path::new(""), beklenen));
    }
}
