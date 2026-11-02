//! PDF : pages en images (lecteur PDF de Windows, `Windows.Data.Pdf`, sans dépendance ni logiciel à installer)
//! et images en PDF (fichier écrit à la main, JPEG intégrés tels quels).

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Au-delà, un PDF n'est converti que sur ses premières pages (mémoire : toutes les pages sont gardées avant l'écriture)
const MAX_PAGES: u32 = 300;
/// Les PDF sont dessinés à 96 points par pouce ; ×2 = 192 ppp (A4 : environ 1 600 × 2 250 px)
const SCALE: f32 = 2.0;
/// Les images d'un PDF créé sont placées à 150 ppp
const PDF_DPI: f64 = 150.0;

/// Dessine les pages du PDF en PNG (fond blanc).
fn render_pages(src: &Path) -> Result<Vec<Vec<u8>>, String> {
    use windows::{
        core::HSTRING,
        Data::Pdf::{PdfDocument, PdfPageRenderOptions},
        Storage::{
            Streams::{DataReader, InMemoryRandomAccessStream},
            StorageFile,
        },
        UI::Color,
        Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
    };
    let e = |x: windows::core::Error| x.message();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(src.to_string_lossy().as_ref()))
        .and_then(|a| a.join())
        .map_err(|x| format!("fichier illisible ({})", e(x)))?;
    let doc = PdfDocument::LoadFromFileAsync(&file)
        .and_then(|a| a.join())
        .map_err(|_| "PDF protégé par un mot de passe ou illisible".to_string())?;
    let count = doc.PageCount().map_err(e)?.min(MAX_PAGES);
    let mut pages = Vec::new();
    for i in 0..count {
        let page = doc.GetPage(i).map_err(e)?;
        let size = page.Size().map_err(e)?;
        // Plus grand côté limité à 8 000 px (affiches, plans)
        let k = SCALE.min(8000.0 / size.Width.max(size.Height).max(1.0));
        let opts = PdfPageRenderOptions::new().map_err(e)?;
        opts.SetDestinationWidth((size.Width * k).round().max(1.0) as u32).map_err(e)?;
        opts.SetDestinationHeight((size.Height * k).round().max(1.0) as u32).map_err(e)?;
        opts.SetBackgroundColor(Color { A: 255, R: 255, G: 255, B: 255 }).map_err(e)?;
        let stream = InMemoryRandomAccessStream::new().map_err(e)?;
        page.RenderWithOptionsToStreamAsync(&stream, &opts).and_then(|a| a.join()).map_err(e)?;
        let len = stream.Size().map_err(e)? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(e)?).map_err(e)?;
        reader.LoadAsync(len).and_then(|a| a.join()).map_err(e)?;
        let mut buf = vec![0u8; len as usize];
        reader.ReadBytes(&mut buf).map_err(e)?;
        pages.push(buf);
    }
    Ok(pages)
}

/// Convertit un PDF en images PNG / JPG / WebP à côté de lui : une page = `nom.png`, plusieurs pages = un dossier `nom`
/// contenant `nom - page 01.png`… `name_for(dossier, nom, est_un_dossier)` donne un nom libre. Renvoie ce qui a été créé
/// (le fichier, ou le dossier).
pub fn pdf_to_images(src: &Path, format: &str, name_for: impl Fn(&Path, &str, bool) -> PathBuf) -> Result<PathBuf, String> {
    if !matches!(format, "png" | "jpg" | "webp") {
        return Err("format inconnu".into());
    }
    let pages = render_pages(src)?;
    if pages.is_empty() {
        return Err("le PDF n'a aucune page".into());
    }
    let encode = |png: &[u8]| -> Result<Vec<u8>, String> {
        if format == "png" {
            return Ok(png.to_vec());
        }
        let img = image::load_from_memory(png).map_err(|e| e.to_string())?;
        crate::fileops::encode(&img, format, 92)
    };
    let dir = src.parent().unwrap_or(Path::new("."));
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    if pages.len() == 1 {
        let out = name_for(dir, &format!("{stem}.{format}"), false);
        fs::write(&out, encode(&pages[0])?).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    let folder = name_for(dir, &stem, true);
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let width = pages.len().to_string().len().max(2);
    for (i, p) in pages.iter().enumerate() {
        let name = format!("{stem} - page {:0w$}.{format}", i + 1, w = width);
        fs::write(folder.join(name), encode(p)?).map_err(|e| e.to_string())?;
    }
    Ok(folder)
}

/// Assemble des images en un seul PDF (une page par image, dans l'ordre donné). Chaque image est réencodée en JPEG q92
/// sur fond blanc puis intégrée telle quelle (`DCTDecode`) ; la page a la taille de l'image à 150 ppp.
pub fn images_to_pdf(srcs: &[PathBuf], name_for: impl Fn(&Path, &str) -> PathBuf) -> Result<PathBuf, String> {
    let first = srcs.first().ok_or("aucune image")?;
    let mut pages = Vec::new();
    for s in srcs {
        let img = crate::heif::open_any(s).map_err(|e| format!("{} : {e}", s.file_name().unwrap_or_default().to_string_lossy()))?;
        let jpeg = crate::fileops::encode(&img, "jpg", 92)?;
        pages.push((img.width(), img.height(), jpeg));
    }
    let bytes = build_pdf(&pages);
    let stem = first.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "images".into());
    let name = if srcs.len() > 1 { format!("{stem} (+{}).pdf", srcs.len() - 1) } else { format!("{stem}.pdf") };
    let out = name_for(first.parent().unwrap_or(Path::new(".")), &name);
    fs::write(&out, bytes).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Fichier PDF 1.4 : catalogue, liste des pages, puis pour chaque image une page, son contenu et l'image JPEG.
fn build_pdf(pages: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let mut object = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n", offsets.len());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    object(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: String = (0..pages.len()).map(|i| format!("{} 0 R ", 3 + 3 * i)).collect();
    object(&mut out, format!("<< /Type /Pages /Kids [ {kids}] /Count {} >>", pages.len()).as_bytes());
    for (i, (w, h, jpeg)) in pages.iter().enumerate() {
        let (pw, ph) = ((*w as f64 * 72.0 / PDF_DPI).max(1.0), (*h as f64 * 72.0 / PDF_DPI).max(1.0));
        let (content, image) = (4 + 3 * i, 5 + 3 * i);
        object(
            &mut out,
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 {image} 0 R >> >> /Contents {content} 0 R >>").as_bytes(),
        );
        let draw = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im0 Do Q");
        object(&mut out, format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).as_bytes());
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .into_bytes();
        body.extend_from_slice(jpeg);
        body.extend_from_slice(b"\nendstream");
        object(&mut out, &body);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for o in &offsets {
        let _ = write!(out, "{o:010} 00000 n \n");
    }
    let _ = write!(out, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Aller-retour : images → PDF (écrit par nous) → pages dessinées par Windows, aux bonnes proportions.
    #[test]
    fn aller_retour() {
        let dir = std::env::temp_dir().join(format!("kane-pdf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut srcs = Vec::new();
        for (i, (w, h)) in [(600u32, 400u32), (300, 500)].into_iter().enumerate() {
            let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 90]));
            let p = dir.join(format!("img{i}.png"));
            img.save(&p).unwrap();
            srcs.push(p);
        }
        let free = |d: &Path, n: &str| d.join(n);
        let pdf = images_to_pdf(&srcs, free).unwrap();
        assert!(pdf.ends_with("img0 (+1).pdf"));
        let pages = render_pages(&pdf).unwrap();
        assert_eq!(pages.len(), 2);
        let (a, b) = (image::load_from_memory(&pages[0]).unwrap(), image::load_from_memory(&pages[1]).unwrap());
        assert!((a.width() as f64 / a.height() as f64 - 1.5).abs() < 0.02, "{}x{}", a.width(), a.height());
        assert!((b.width() as f64 / b.height() as f64 - 0.6).abs() < 0.02, "{}x{}", b.width(), b.height());
        let named = |d: &Path, n: &str, _dir: bool| d.join(n);
        let out = pdf_to_images(&pdf, "jpg", named).unwrap();
        assert!(out.is_dir());
        assert_eq!(fs::read_dir(&out).unwrap().count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}
