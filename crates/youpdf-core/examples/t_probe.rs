use youpdf_core::{validate_language, fold_case};
fn main(){
  for l in ["fra","eng","eng+fra","script/Latn","osd","", "FR", "fra; rm -rf /", "--configfile=/tmp/x", "fra/../etc/passwd", "a".repeat(50).as_str(), "fra+"] {
    println!("{:30} -> {:?}", l, validate_language(l).map(|s| s.to_string()).map_err(|e| e.to_string().chars().take(28).collect::<String>()));
  }
  println!("fold EGLISE = {}", fold_case("EGLISE"));
  println!("fold ÉGLISE ÇA = {}", fold_case("ÉGLISE ÇA Ñ"));
  println!("len ok = {}", fold_case("Éẛİ").len() == "Éẛİ".len());
}
