use eframe::egui;
use std::process::Command;
use rfd::FileDialog;

struct VideoMergerApp {
    video_path: String,
    audio_paths: Vec<String>,
    sub_paths: Vec<String>,
    output_path: String,
    
    reencode_video: bool,
    video_codec: String,
    status_message: String,
}

impl Default for VideoMergerApp {
    fn default() -> Self {
        Self {
            video_path: String::new(),
            audio_paths: Vec::new(),
            sub_paths: Vec::new(),
            output_path: String::from("output.mkv"),
            reencode_video: false,
            video_codec: String::from("copy"),
            status_message: String::from("Sẵn sàng. Vui lòng chọn các file đầu vào."),
        }
    }
}

impl VideoMergerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // --- NẠP FONT TIẾNG VIỆT TỪ HỆ THỐNG WINDOWS ---
        let mut fonts = egui::FontDefinitions::default();
        
        // Thêm font Segoe UI từ thư mục Windows Fonts để hỗ trợ tiếng Việt đầy đủ
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
            fonts.font_data.insert(
                "segoe_ui".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            
            // Đưa font vào danh sách ưu tiên cho Proportional
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "segoe_ui".to_owned());
        }

        cc.egui_ctx.set_fonts(fonts);
        Self::default()
    }
}

impl eframe::App for VideoMergerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Tăng kích thước style chung cho đỡ thô
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        ctx.set_style(style);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(5.0);
            ui.heading("🎬 Trình Gộp Video & Phụ Đề (Rust)");
            ui.separator();
            ui.add_space(5.0);

            // --- 1. CHỌN VIDEO GỐC ---
            ui.horizontal(|ui| {
                if ui.button("📁 Chọn Video Gốc (.mp4)").clicked() {
                    if let Some(path) = FileDialog::new().pick_file() {
                        self.video_path = path.display().to_string();
                    }
                }
                let display_text = if self.video_path.is_empty() {
                    "Chưa chọn file nào".to_string()
                } else {
                    self.video_path.clone()
                };
                ui.label(display_text);
            });

            ui.add_space(5.0);

            // --- 2. CHỌN ÂM THANH ---
            ui.horizontal(|ui| {
                if ui.button("🎵 Thêm File Audio (MP3/AAC)").clicked() {
                    if let Some(path) = FileDialog::new().pick_file() {
                        self.audio_paths.push(path.display().to_string());
                    }
                }
                ui.label(format!("Đã chọn: {} audio track", self.audio_paths.len()));
            });

            for (i, audio) in self.audio_paths.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.indent(format!("audio_{}", i), |ui| {
                        ui.label(format!("• Track #{}: {}", i + 1, audio));
                    });
                });
            }

            ui.add_space(5.0);

            // --- 3. CHỌN PHỤ ĐỀ ---
            ui.horizontal(|ui| {
                if ui.button("💬 Thêm Phụ Đề (.vtt)").clicked() {
                    if let Some(path) = FileDialog::new().pick_file() {
                        self.sub_paths.push(path.display().to_string());
                    }
                }
                ui.label(format!("Đã chọn: {} sub track", self.sub_paths.len()));
            });

            for (i, sub) in self.sub_paths.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.indent(format!("sub_{}", i), |ui| {
                        ui.label(format!("• Sub #{}: {}", i + 1, sub));
                    });
                });
            }

            ui.separator();

            // --- 4. TÙY CHỌN NÂNG CAO ---
            ui.collapsing("⚙ Tùy chọn nâng cao (Codec & Chất lượng)", |ui| {
                ui.checkbox(&mut self.reencode_video, "Encode lại video (Dùng khi cần thay đổi chất lượng/codec)");
                if self.reencode_video {
                    ui.horizontal(|ui| {
                        ui.label("Video Codec:");
                        ui.text_edit_singleline(&mut self.video_codec);
                    });
                } else {
                    self.video_codec = String::from("copy");
                }
            });

            ui.separator();
            ui.add_space(5.0);

            // --- 5. NÚT THỰC THI ---
            if ui.button("🚀 Bắt đầu Quá Trình Merge").clicked() {
                if self.video_path.is_empty() {
                    self.status_message = String::from("❌ Lỗi: Chưa chọn video gốc!");
                } else {
                    match self.run_ffmpeg_merge() {
                        Ok(_) => self.status_message = String::from("✅ Gộp file thành công: output.mkv"),
                        Err(e) => self.status_message = format!("❌ Lỗi: {}", e),
                    }
                }
            }

            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(format!("Trạng thái: {}", self.status_message));
            });
        });
    }
}

impl VideoMergerApp {
    fn run_ffmpeg_merge(&self) -> Result<(), String> {
        let mut cmd = Command::new("ffmpeg");

        cmd.arg("-i").arg(&self.video_path);

        for audio in &self.audio_paths {
            cmd.arg("-i").arg(audio);
        }

        for sub in &self.sub_paths {
            cmd.arg("-i").arg(sub);
        }

        cmd.arg("-map").arg("0:v:0");

        let total_audio_inputs = 1 + self.audio_paths.len();
        for i in 0..total_audio_inputs {
            cmd.arg("-map").arg(format!("{}:a:0", i));
        }

        let base_sub_index = total_audio_inputs;
        for (i, _) in self.sub_paths.iter().enumerate() {
            cmd.arg("-map").arg(format!("{}:s:0", base_sub_index + i));
        }

        cmd.arg("-c:v").arg(&self.video_codec);
        cmd.arg("-c:a").arg("aac");
        cmd.arg("-c:s").arg("webvtt");

        cmd.arg("-metadata:s:a:0").arg("language=chi");
        if !self.audio_paths.is_empty() {
            cmd.arg("-metadata:s:a:1").arg("language=vie");
        }
        if !self.sub_paths.is_empty() {
            cmd.arg("-metadata:s:s:0").arg("language=chi");
        }
        if self.sub_paths.len() > 1 {
            cmd.arg("-metadata:s:s:1").arg("language=vie");
        }

        cmd.arg("output.mkv");

        let status = cmd.status().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(String::from("FFmpeg thực thi thất bại, kiểm tra lại định dạng file."))
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([700.0, 500.0]), // Mở rộng kích thước cửa sổ mặc định
        ..Default::default()
    };
    
    eframe::run_native(
        "Video Merger GUI (Rust)",
        options,
        Box::new(|cc| Ok(Box::new(VideoMergerApp::new(cc)))),
    )
}