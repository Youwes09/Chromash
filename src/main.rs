use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use image::{imageops::FilterType, GenericImageView, ImageReader};

#[derive(Debug)]
pub enum ChromashError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Process(String),
    NotFound(String),
    General(String),
}

impl From<std::io::Error> for ChromashError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}
impl From<serde_json::Error> for ChromashError {
    fn from(e: serde_json::Error) -> Self { Self::Json(e) }
}
impl std::fmt::Display for ChromashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Json(e) => write!(f, "JSON error: {}", e),
            Self::Process(e) => write!(f, "Process failed: {}", e),
            Self::NotFound(e) => write!(f, "Not found: {}", e),
            Self::General(e) => write!(f, "Error: {}", e),
        }
    }
}
impl std::error::Error for ChromashError {}
type Result<T> = std::result::Result<T, ChromashError>;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum ColorMode { Light, Dark }

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum SchemeType {
    Content,
    Expressive,
    Fidelity,
    FruitSalad,
    Monochrome,
    Neutral,
    Rainbow,
    TonalSpot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetMetadata {
    pub name: String,
    pub created: u64,
    pub modified: u64,
    pub source: Option<String>,
    pub wallpaper: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentTheme {
    pub source: String,
    pub timestamp: u64,
    pub preset_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ThemeOptions {
    pub mode: Option<ColorMode>,
    pub scheme: Option<SchemeType>,
    pub save_preset: bool,
    pub preset_name: Option<String>,
}

impl Default for ThemeOptions {
    fn default() -> Self {
        Self { mode: None, scheme: None, save_preset: false, preset_name: None }
    }
}

impl ColorMode {
    fn as_str(&self) -> &'static str {
        match self { Self::Light => "light", Self::Dark => "dark" }
    }
    fn from_brightness(r: u8, g: u8, b: u8) -> Self {
        if (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000 > 128 { 
            Self::Light 
        } else { 
            Self::Dark 
        }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

impl SchemeType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Content => "scheme-content",
            Self::Expressive => "scheme-expressive",
            Self::Fidelity => "scheme-fidelity",
            Self::FruitSalad => "scheme-fruit-salad",
            Self::Monochrome => "scheme-monochrome",
            Self::Neutral => "scheme-neutral",
            Self::Rainbow => "scheme-rainbow",
            Self::TonalSpot => "scheme-tonal-spot",
        }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace("-", "").replace("_", "").as_str() {
            "content" | "schemecontent" => Some(Self::Content),
            "expressive" | "schemeexpressive" => Some(Self::Expressive),
            "fidelity" | "schemefidelity" => Some(Self::Fidelity),
            "fruitsalad" | "schemefruitsalad" => Some(Self::FruitSalad),
            "monochrome" | "schememonochrome" => Some(Self::Monochrome),
            "neutral" | "schemeneutral" => Some(Self::Neutral),
            "rainbow" | "schemerainbow" => Some(Self::Rainbow),
            "tonalspot" | "schemetonalspot" => Some(Self::TonalSpot),
            _ => None,
        }
    }
    fn from_chroma(r: u8, g: u8, b: u8) -> Self {
        let chroma = r.max(g).max(b) - r.min(g).min(b);
        if chroma < 30 {
            Self::Neutral
        } else if chroma < 60 {
            Self::TonalSpot
        } else {
            Self::Expressive
        }
    }
}

pub struct Config;

impl Config {
    fn home() -> PathBuf {
        env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"))
    }
    fn config_dir() -> PathBuf {
        Self::home().join(".config/chromash")
    }
    fn wallpaper_dir() -> PathBuf {
        env::var("XDG_PICTURES_DIR")
            .map(|p| PathBuf::from(p).join("Wallpapers"))
            .unwrap_or_else(|_| Self::home().join("Pictures/Wallpapers"))
    }
    fn hyprpaper_dir() -> PathBuf {
        Self::home().join(".config/hypr/hyprpaper")
    }
    fn hyprpaper_config() -> PathBuf {
        Self::home().join(".config/hypr/hyprpaper.conf")
    }
    fn presets_dir() -> PathBuf {
        Self::config_dir().join("presets")
    }
    fn current_theme_file() -> PathBuf {
        Self::config_dir().join("current_theme.json")
    }
}

pub struct ChromashApi;

impl ChromashApi {
    pub fn new() -> Result<Self> {
        let dirs = [
            Config::config_dir(), 
            Config::presets_dir(), 
            Config::wallpaper_dir(),
            Config::hyprpaper_dir()
        ];
        for dir in &dirs {
            fs::create_dir_all(dir)?;
        }
        Ok(Self)
    }
    
    fn run_command(&self, program: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(program).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(ChromashError::Process(String::from_utf8_lossy(&output.stderr).to_string()))
        }
    }
    
    fn save_current_theme(&self, source: &str, preset_name: Option<String>) -> Result<()> {
        let theme = CurrentTheme {
            source: source.to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            preset_name,
        };
        let content = serde_json::to_string_pretty(&theme)?;
        fs::write(Config::current_theme_file(), content)?;
        Ok(())
    }
    
    pub fn load_current_theme(&self) -> Result<Option<CurrentTheme>> {
        let theme_file = Config::current_theme_file();
        if theme_file.exists() {
            let content = fs::read_to_string(&theme_file)?;
            let theme: CurrentTheme = serde_json::from_str(&content)?;
            Ok(Some(theme))
        } else {
            Ok(None)
        }
    }
    
    pub fn apply_color(&mut self, color: &str, options: ThemeOptions) -> Result<bool> {
        let mode = options.mode.unwrap_or(ColorMode::Light);
        let scheme = options.scheme.unwrap_or(SchemeType::TonalSpot);
        
        let output = Command::new("matugen")
            .args(&["-m", mode.as_str(), "-t", scheme.as_str(), "color", "hex", color])
            .output()?;
        
        if output.status.success() {
            let source = format!("color_{}", color);
            if options.save_preset {
                if let Some(name) = &options.preset_name {
                    self.save_preset(name, Some(source.clone()), None)?;
                    self.save_current_theme(&source, Some(name.clone()))?;
                } else {
                    self.save_current_theme(&source, None)?;
                }
            } else {
                self.save_current_theme(&source, None)?;
            }
            Ok(true)
        } else {
            Err(ChromashError::Process(String::from_utf8_lossy(&output.stderr).to_string()))
        }
    }
    
    pub fn apply_wallpaper(&mut self, path: Option<&str>, extract_colors: bool, options: ThemeOptions) -> Result<bool> {
        let wallpaper_path = self.select_wallpaper(path)?;
        self.set_wallpaper(&wallpaper_path)?;
        
        if extract_colors {
            if let Ok((r, g, b)) = self.get_average_color(&wallpaper_path) {
                let mode = options.mode.unwrap_or_else(|| ColorMode::from_brightness(r, g, b));
                let scheme = options.scheme.unwrap_or_else(|| SchemeType::from_chroma(r, g, b));
                self.apply_image_colors(&wallpaper_path, mode, scheme)?;
                
                let source = format!("wallpaper_{}", wallpaper_path.display());
                
                if options.save_preset {
                    if let Some(name) = &options.preset_name {
                        self.save_preset(name, Some(source.clone()), Some(wallpaper_path.display().to_string()))?;
                        self.save_current_theme(&source, Some(name.clone()))?;
                    } else {
                        self.save_current_theme(&source, None)?;
                    }
                } else {
                    self.save_current_theme(&source, None)?;
                }
            }
        }
        Ok(true)
    }
    
    fn apply_image_colors(&mut self, image_path: &Path, mode: ColorMode, scheme: SchemeType) -> Result<bool> {
        let output = Command::new("matugen")
            .args(&["-m", mode.as_str(), "-t", scheme.as_str(), "image", &image_path.to_string_lossy()])
            .output()?;
        
        if output.status.success() {
            Ok(true)
        } else {
            Err(ChromashError::Process(String::from_utf8_lossy(&output.stderr).to_string()))
        }
    }
    
    fn select_wallpaper(&self, path: Option<&str>) -> Result<PathBuf> {
        if let Some(p) = path {
            let path_buf = if p.starts_with('~') {
                Config::home().join(p.strip_prefix("~/").unwrap_or(p))
            } else {
                PathBuf::from(p)
            };
            if path_buf.is_file() { return Ok(path_buf); }
        }
        
        // Check for existing wallpaper in hyprpaper directory
        let hyprpaper_dir = Config::hyprpaper_dir();
        if hyprpaper_dir.is_dir() {
            for entry in fs::read_dir(&hyprpaper_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                        if ["png", "jpg", "jpeg", "gif", "bmp", "webp"].contains(&ext.to_lowercase().as_str()) {
                            return Ok(path);
                        }
                    }
                }
            }
        }
        
        let wall_dir = Config::wallpaper_dir();
        if wall_dir.is_dir() {
            for entry in fs::read_dir(&wall_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                        if ["png", "jpg", "jpeg", "gif", "bmp", "webp"].contains(&ext.to_lowercase().as_str()) {
                            return Ok(entry.path());
                        }
                    }
                }
            }
        }
        Err(ChromashError::NotFound("No wallpaper found".into()))
    }
    
    fn set_wallpaper(&self, path: &Path) -> Result<()> {
        let hyprpaper_dir = Config::hyprpaper_dir();
        fs::create_dir_all(&hyprpaper_dir)?;
        
        // Copy wallpaper to hyprpaper directory with original extension
        let file_name = path.file_name()
            .ok_or_else(|| ChromashError::General("Invalid file name".into()))?;
        let dest_path = hyprpaper_dir.join(file_name);
        
        // Clean up old wallpapers before copying new one
        self.cleanup_old_wallpapers(&hyprpaper_dir, &dest_path)?;
        
        // Copy the file
        fs::copy(path, &dest_path)?;
        
        // Write hyprpaper.conf with the copied path
        self.write_hyprpaper_config(&dest_path)?;
        
        // Apply wallpaper via hyprctl using the copied path
        let dest_path_str = dest_path.to_string_lossy();
        
        // Unload all and wait
        let _ = self.run_command("hyprctl", &["hyprpaper", "unload", "all"]);
        std::thread::sleep(std::time::Duration::from_millis(200));
        
        // Preload new wallpaper
        self.run_command("hyprctl", &["hyprpaper", "preload", &dest_path_str])?;
        
        // Set on all monitors
        let monitors = self.run_command("hyprctl", &["monitors"])?;
        for line in monitors.lines() {
            if line.starts_with("Monitor") {
                if let Some(monitor) = line.split_whitespace().nth(1) {
                    let wallpaper_arg = format!("{},{}", monitor, dest_path_str);
                    let _ = self.run_command("hyprctl", &["hyprpaper", "wallpaper", &wallpaper_arg]);
                }
            }
        }
        Ok(())
    }
    
    fn cleanup_old_wallpapers(&self, hyprpaper_dir: &Path, keep_path: &Path) -> Result<()> {
        if !hyprpaper_dir.is_dir() {
            return Ok(());
        }
        
        for entry in fs::read_dir(hyprpaper_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            // Skip if it's not a file
            if !entry.file_type()?.is_file() {
                continue;
            }
            
            // Skip if it's the one we're about to copy
            if path == keep_path {
                continue;
            }
            
            // Check if it's an image file
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ["png", "jpg", "jpeg", "gif", "bmp", "webp"].contains(&ext.to_lowercase().as_str()) {
                    // Delete old wallpaper
                    let _ = fs::remove_file(&path);
                }
            }
        }
        Ok(())
    }
    
    fn write_hyprpaper_config(&self, wallpaper_path: &Path) -> Result<()> {
        let config_path = Config::hyprpaper_config();
        let wallpaper_str = wallpaper_path.to_string_lossy();
        
        let config_content = format!(
            "# hyprpaper configuration - managed by chromash\n\
             preload = {}\n\
             wallpaper = ,{}\n\
             \n\
             # If you have specific monitor configurations, add them below:\n\
             # wallpaper = HDMI-A-1,{}\n\
             # wallpaper = eDP-1,{}\n",
            wallpaper_str, wallpaper_str, wallpaper_str, wallpaper_str
        );
        
        fs::write(&config_path, config_content)?;
        Ok(())
    }
    
    fn get_average_color(&self, path: &Path) -> Result<(u8, u8, u8)> {
        let img = ImageReader::open(path)?.with_guessed_format()?.decode()
            .map_err(|e| ChromashError::General(format!("Failed to decode: {}", e)))?;
            
        let (width, height) = img.dimensions();
        let resized_img = if width > 128 || height > 128 {
            let scale = 128.0 / width.max(height) as f64;
            let new_w = (width as f64 * scale).round().max(1.0) as u32;
            let new_h = (height as f64 * scale).round().max(1.0) as u32;
            img.resize_exact(new_w, new_h, FilterType::CatmullRom)
        } else {
            img
        };
        
        let rgb_img = resized_img.into_rgb8();
        let mut color_counts: HashMap<[u8; 3], u32> = HashMap::new();
        
        for pixel in rgb_img.pixels() {
            let quantized = [(pixel[0] / 16) * 16, (pixel[1] / 16) * 16, (pixel[2] / 16) * 16];
            *color_counts.entry(quantized).or_insert(0) += 1;
        }
        
        let mut best_color = [128u8, 128u8, 128u8];
        let mut best_score = 0.0;
        
        for (&color, &count) in &color_counts {
            let [r, g, b] = color;
            let chroma = r.max(g).max(b) - r.min(g).min(b);
            let lightness = (r as u32 + g as u32 + b as u32) / 3;
            
            let chroma_score = if chroma > 30 { 1.0 } else { chroma as f64 / 30.0 };
            let lightness_score = if lightness > 50 && lightness < 200 { 1.0 } else { 0.5 };
            let frequency_score = (count as f64).ln();
            
            let total_score = chroma_score * lightness_score * frequency_score;
            if total_score > best_score {
                best_score = total_score;
                best_color = color;
            }
        }
        
        Ok((best_color[0], best_color[1], best_color[2]))
    }
    
    pub fn list_presets(&self) -> Result<Vec<PresetMetadata>> {
        let presets_dir = Config::presets_dir();
        if !presets_dir.exists() { return Ok(Vec::new()); }
        
        let mut presets = Vec::new();
        for entry in fs::read_dir(&presets_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let metadata_file = path.join("metadata.json");
                if metadata_file.exists() {
                    if let Ok(content) = fs::read_to_string(&metadata_file) {
                        if let Ok(metadata) = serde_json::from_str::<PresetMetadata>(&content) {
                            presets.push(metadata);
                        }
                    }
                }
            }
        }
        presets.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(presets)
    }
    
    pub fn apply_preset(&mut self, name: &str) -> Result<bool> {
        let preset_dir = self.get_preset_dir(name)?;
        let metadata_file = preset_dir.join("metadata.json");
        
        if !metadata_file.exists() {
            return Err(ChromashError::NotFound(format!("Preset metadata for: {}", name)));
        }
        
        let content = fs::read_to_string(&metadata_file)?;
        let metadata: PresetMetadata = serde_json::from_str(&content)?;
        
        if let Some(source) = &metadata.source {
            if source.starts_with("color_") {
                let color = source.strip_prefix("color_").unwrap_or("ffffff");
                return self.apply_color(color, ThemeOptions::default());
            } else if source.starts_with("wallpaper_") {
                let wallpaper_path = source.strip_prefix("wallpaper_").unwrap_or("");
                if Path::new(wallpaper_path).exists() {
                    return self.apply_wallpaper(Some(wallpaper_path), true, ThemeOptions::default());
                }
            }
        }
        
        if let Some(wallpaper) = &metadata.wallpaper {
            if Path::new(wallpaper).exists() {
                return self.apply_wallpaper(Some(wallpaper), true, ThemeOptions::default());
            }
        }
        
        Err(ChromashError::NotFound(format!("Unable to apply preset: {}", name)))
    }
    
    pub fn save_preset(&self, name: &str, source: Option<String>, wallpaper: Option<String>) -> Result<bool> {
        let preset_dir = Config::presets_dir().join(self.sanitize_name(name));
        fs::create_dir_all(&preset_dir)?;
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let metadata = PresetMetadata {
            name: name.to_string(),
            created: now,
            modified: now,
            source,
            wallpaper,
        };
        
        let metadata_file = preset_dir.join("metadata.json");
        let content = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_file, content)?;
        Ok(true)
    }
    
    pub fn delete_preset(&self, name: &str) -> Result<bool> {
        let preset_dir = Config::presets_dir().join(self.sanitize_name(name));
        if preset_dir.exists() {
            fs::remove_dir_all(&preset_dir)?;
            Ok(true)
        } else {
            for preset in self.list_presets()? {
                if preset.name == name {
                    let found_dir = Config::presets_dir().join(self.sanitize_name(&preset.name));
                    if found_dir.exists() {
                        fs::remove_dir_all(&found_dir)?;
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
    }
    
    fn get_preset_dir(&self, name: &str) -> Result<PathBuf> {
        let sanitized_dir = Config::presets_dir().join(self.sanitize_name(name));
        if sanitized_dir.exists() {
            return Ok(sanitized_dir);
        }
        
        for preset in self.list_presets()? {
            if preset.name == name {
                let dir = Config::presets_dir().join(self.sanitize_name(&preset.name));
                if dir.exists() {
                    return Ok(dir);
                }
            }
        }
        Err(ChromashError::NotFound(format!("Preset directory for: {}", name)))
    }
    
    fn sanitize_name(&self, name: &str) -> String {
        name.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ' ')
            .collect::<String>()
            .replace(' ', "_")
    }
}

fn format_timestamp(timestamp: u64) -> String {
    let datetime = UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
    format!("{:?}", datetime)
}

fn parse_theme_options(args: &[String], start_idx: usize) -> (ThemeOptions, Vec<String>) {
    let mut options = ThemeOptions::default();
    let mut remaining_args = Vec::new();
    let mut i = start_idx;
    
    while i < args.len() {
        match args[i].as_str() {
            "--mode" | "-m" if i + 1 < args.len() => {
                if let Some(mode) = ColorMode::from_str(&args[i + 1]) {
                    options.mode = Some(mode);
                    i += 2;
                    continue;
                }
            }
            "--scheme" | "-s" if i + 1 < args.len() => {
                if let Some(scheme) = SchemeType::from_str(&args[i + 1]) {
                    options.scheme = Some(scheme);
                    i += 2;
                    continue;
                }
            }
            "--save-preset" => {
                options.save_preset = true;
                if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                    options.preset_name = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                remaining_args.push(args[i].clone());
                i += 1;
            }
        }
    }
    (options, remaining_args)
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 || args[1] == "help" {
        show_help();
        return Ok(());
    }
    
    let mut api = ChromashApi::new()?;
    
    match args[1].as_str() {
        "color" => {
            let (options, _) = parse_theme_options(&args, 3);
            api.apply_color(&args[2], options)?;
            println!("Applied color theme: {}", args[2]);
        }
        "wallpaper" => {
            let path = if args.len() > 2 { Some(args[2].as_str()) } else { None };
            let (options, _) = parse_theme_options(&args, if path.is_some() { 3 } else { 2 });
            api.apply_wallpaper(path, true, options)?;
            println!("Applied wallpaper and extracted colors");
        }
        "wallpaper-only" => {
            api.apply_wallpaper(Some(&args[2]), false, ThemeOptions::default())?;
            println!("Set wallpaper: {}", args[2]);
        }
        "presets" => {
            let presets = api.list_presets()?;
            if presets.is_empty() {
                println!("No saved presets found");
            } else {
                for preset in presets {
                    println!("{} ({})", preset.name, format_timestamp(preset.modified));
                }
            }
        }
        "preset" => {
            match args[2].as_str() {
                "apply" => {
                    api.apply_preset(&args[3])?;
                    println!("Applied preset: {}", args[3]);
                }
                "save" => {
                    api.save_preset(&args[3], None, None)?;
                    println!("Saved preset: {}", args[3]);
                }
                "delete" => {
                    if api.delete_preset(&args[3])? {
                        println!("Deleted preset: {}", args[3]);
                    } else {
                        println!("Preset not found: {}", args[3]);
                    }
                }
                _ => eprintln!("Unknown preset command: {}", args[2]),
            }
        }
        "theme" => {
            if let Ok(Some(current)) = api.load_current_theme() {
                println!("Source: {}", current.source);
                println!("Time: {}", format_timestamp(current.timestamp));
                if let Some(preset) = current.preset_name {
                    println!("Preset: {}", preset);
                }
            } else {
                println!("No theme info");
            }
        }
        _ => eprintln!("Unknown command: {}", args[1]),
    }
    Ok(())
}

fn show_help() {
    println!("Chromash - Dynamic Theme Manager\n");
    println!("USAGE: chromash <command> [args]\n");
    println!("COMMANDS:");
    println!("  color <hex> [--mode light|dark] [--scheme type] [--save-preset name]");
    println!("  wallpaper [path] [options]     - Set wallpaper and extract colors");
    println!("  wallpaper-only <path>          - Set wallpaper only");
    println!("  presets                        - List presets");
    println!("  preset apply|save|delete <name>");
    println!("  theme                          - Show current theme");
    println!("  help                           - Show help\n");
    println!("SCHEME TYPES:");
    println!("  content, expressive, fidelity, fruit-salad, monochrome,");
    println!("  neutral, rainbow, tonal-spot");
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}