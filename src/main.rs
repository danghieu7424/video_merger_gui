use eframe::egui;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use rfd::FileDialog;

struct VideoMergerApp {
    video_path: String,
    audio_paths: Vec<String>,
    sub_paths: Vec<String>,
    output_path: String,
    
    reencode_video: bool,
    video_codec: String,
    status_message: String,
    is_processing: bool,

    // Kênh truyền tin (channel) giữa background thread và UI thread
    rx: Receiver<Result<String, String>>,
    tx: Sender<Result<String, String>>,
}

impl Default for VideoMergerApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            video_path: String::new(),
            audio_paths: Vec::new(),
            sub_paths: Vec::new(),
            output_path: String::from("output.mkv"),
            reencode_video: false,
            video_codec: String::from("copy"),
            status_message: String::from("Sẵn sàng. Vui lòng chọn các file đầu vào."),
            is_processing: false,
            rx,
            tx,
        }
    }
}

impl VideoMergerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // --- NẠP FONT TIẾNG VIỆT TỪ HỆ THỐNG WINDOWS ---
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
            fonts.font_data.insert(
                "segoe_ui".to_owned(),
                egui::FontData::from_owned(font_data),
            );
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
        // Kiểm tra kết quả từ background thread trả về
        if let Ok(result) = self.rx.try_recv() {
            self.is_processing = false;
            match result {
                Ok(msg) => self.status_message = format!("✅ {}", msg),
                Err(err) => self.status_message = format!("❌ Lỗi: {}", err),
            }
        }

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        ctx.set_style(style);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(5.0);
            ui.heading("🎬 Trình Gộp Video & Phụ Đề Đa Ngôn Ngữ (Rust)");
            ui.separator();
            ui.add_space(5.0);

            // --- 1. CHỌN VIDEO GỐC ---
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.is_processing, egui::Button::new("📁 Chọn Video Gốc (.mp4)")).clicked() {
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

            // --- 2. CHỌN ÂM THANH (Hỗ trợ chọn nhiều file cùng lúc) ---
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.is_processing, egui::Button::new("🎵 Thêm File Audio (MP3/AAC/MP4)")).clicked() {
                    if let Some(paths) = FileDialog::new().pick_files() {
                        for path in paths {
                            self.audio_paths.push(path.display().to_string());
                        }
                    }
                }
                ui.label(format!("Đã chọn: {} audio track", self.audio_paths.len()));
                if ui.add_enabled(!self.is_processing && !self.audio_paths.is_empty(), egui::Button::new("🗑 Xóa hết")).clicked() {
                    self.audio_paths.clear();
                }
            });

            for (i, audio) in self.audio_paths.iter().enumerate() {
                let lang = Self::detect_language_from_filename(audio);
                ui.indent(format!("audio_{}", i), |ui| {
                    ui.label(format!("• Track bổ sung #{0} [Mã ISO: {1}]: {2}", i + 1, lang.to_uppercase(), audio));
                });
            }

            ui.add_space(5.0);

            // --- 3. CHỌN PHỤ ĐỀ (Hỗ trợ chọn nhiều file .vtt / .srt) ---
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.is_processing, egui::Button::new("💬 Thêm Phụ Đề (.vtt / .srt)")).clicked() {
                    if let Some(paths) = FileDialog::new().pick_files() {
                        for path in paths {
                            self.sub_paths.push(path.display().to_string());
                        }
                    }
                }
                ui.label(format!("Đã chọn: {} sub track", self.sub_paths.len()));
                if ui.add_enabled(!self.is_processing && !self.sub_paths.is_empty(), egui::Button::new("🗑 Xóa hết")).clicked() {
                    self.sub_paths.clear();
                }
            });

            for (i, sub) in self.sub_paths.iter().enumerate() {
                let lang = Self::detect_language_from_filename(sub);
                ui.indent(format!("sub_{}", i), |ui| {
                    ui.label(format!("• Phụ đề #{0} [Mã ISO: {1}]: {2}", i + 1, lang.to_uppercase(), sub));
                });
            }

            ui.separator();

            // --- 4. TÙY CHỌN NÂNG CAO ---
            ui.collapsing("⚙ Tùy chọn nâng cao (Codec & Đường dẫn lưu)", |ui| {
                ui.checkbox(&mut self.reencode_video, "Encode lại video (Dùng khi cần thay đổi chất lượng/codec)");
                if self.reencode_video {
                    ui.horizontal(|ui| {
                        ui.label("Video Codec:");
                        ui.text_edit_singleline(&mut self.video_codec);
                    });
                } else {
                    self.video_codec = String::from("copy");
                }

                ui.horizontal(|ui| {
                    ui.label("File xuất ra:");
                    ui.text_edit_singleline(&mut self.output_path);
                    if ui.button("📂 Lưu tại...").clicked() {
                        if let Some(path) = FileDialog::new().set_file_name(&self.output_path).save_file() {
                            self.output_path = path.display().to_string();
                        }
                    }
                });
            });

            ui.separator();
            ui.add_space(5.0);

            // --- 5. NÚT THỰC THI (DISABLE KHI CHƯA ĐỦ ĐIỀU KIỆN HOẶC ĐANG XỬ LÝ) ---
            // Điều kiện hợp lệ: Phải có video gốc và ít nhất 1 audio hoặc 1 phụ đề
            let is_input_ready = !self.video_path.is_empty() && (!self.audio_paths.is_empty() || !self.sub_paths.is_empty());
            let can_start = is_input_ready && !self.is_processing;

            let button_text = if self.is_processing {
                "⏳ Đang gộp file (Vui lòng đợi)..."
            } else {
                "🚀 Bắt đầu Quá Trình Merge"
            };

            if ui.add_enabled(can_start, egui::Button::new(button_text)).clicked() {
                self.is_processing = true;
                self.status_message = String::from("Đang chạy FFmpeg ngầm trong hệ thống...");

                let tx = self.tx.clone();
                let video_path = self.video_path.clone();
                let audio_paths = self.audio_paths.clone();
                let sub_paths = self.sub_paths.clone();
                let video_codec = self.video_codec.clone();
                let output_path = self.output_path.clone();

                // Spawning background worker thread để bảo vệ UI thread không bị treo (freeze) hay crash do OS
                std::thread::spawn(move || {
                    let result = Self::execute_ffmpeg_merge(
                        &video_path,
                        &audio_paths,
                        &sub_paths,
                        &video_codec,
                        &output_path,
                    );
                    let _ = tx.send(result);
                });
            }

            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(format!("Trạng thái: {}", self.status_message));
            });
        });
    }
}

/****
 * Thực thi lệnh FFmpeg trong luồng Worker độc lập để tránh đóng băng (freeze/crash) giao diện GUI.
 * Đầu vào: Đường dẫn video, danh sách audio, danh sách subtitle, codec, output path.
 * Đầu ra: Result<String, String> báo cáo kết quả chi tiết cho UI.
 ****/
impl VideoMergerApp {
    fn execute_ffmpeg_merge(
        video_path: &str,
        audio_paths: &[String],
        sub_paths: &[String],
        video_codec: &str,
        output_path: &str,
    ) -> Result<String, String> {
        let mut cmd = Command::new("ffmpeg");

        // Ghi đè file nếu đã tồn tại (-y) tránh việc ffmpeg dừng đợi prompt stdin
        cmd.arg("-y");

        // 1. Khai báo lần lượt các input file vào FFmpeg
        cmd.arg("-i").arg(video_path);

        for audio in audio_paths {
            cmd.arg("-i").arg(audio);
        }

        for sub in sub_paths {
            cmd.arg("-i").arg(sub);
        }

        // 2. Map Stream tự động chính xác theo thứ tự input
        // Map hình ảnh từ video gốc
        cmd.arg("-map").arg("0:v:0");

        // Map audio: bắt đầu từ audio của video gốc (index 0) đến các audio bổ sung tiếp theo
        let total_audio_inputs = 1 + audio_paths.len();
        for i in 0..total_audio_inputs {
            cmd.arg("-map").arg(format!("{}:a:0", i));
        }

        // Map subtitle: bắt đầu từ index ngay sau các file audio
        let base_sub_index = total_audio_inputs;
        for (i, _) in sub_paths.iter().enumerate() {
            cmd.arg("-map").arg(format!("{}:s:0", base_sub_index + i));
        }

        // 3. Cấu hình Codec đầu ra
        cmd.arg("-c:v").arg(video_codec);
        cmd.arg("-c:a").arg("aac");
        cmd.arg("-c:s").arg("webvtt");

        // 4. Gán Metadata ngôn ngữ động cho Audio gốc
        let video_audio_lang = Self::detect_language_from_filename(video_path);
        cmd.arg("-metadata:s:a:0").arg(format!("language={}", video_audio_lang));

        // 5. Gán Metadata ngôn ngữ động cho các Audio bổ sung (index từ 1 trở đi)
        for (i, audio_path) in audio_paths.iter().enumerate() {
            let lang = Self::detect_language_from_filename(audio_path);
            cmd.arg(format!("-metadata:s:a:{}", i + 1)).arg(format!("language={}", lang));
        }

        // 6. Gán Metadata ngôn ngữ động cho Subtitle streams (index từ 0 trở đi)
        for (i, sub_path) in sub_paths.iter().enumerate() {
            let lang = Self::detect_language_from_filename(sub_path);
            cmd.arg(format!("-metadata:s:s:{}", i)).arg(format!("language={}", lang));
        }

        cmd.arg(output_path);

        let output = cmd.output().map_err(|e| format!("Không thể chạy FFmpeg: {}", e))?;
        if output.status.success() {
            Ok(format!("Gộp file hoàn tất thành công -> {}", output_path))
        } else {
            let stderr_msg = String::from_utf8_lossy(&output.stderr);
            Err(format!("FFmpeg lỗi:\n{}", stderr_msg.chars().take(400).collect::<String>()))
        }
    }

    // Hàm thông minh phân tích tên file để tự động gán mã ngôn ngữ chuẩn quốc tế (ISO 639-2)
    fn detect_language_from_filename(path: &str) -> &'static str {
        let lower = path.to_lowercase();
        if lower.contains(".vi.") || lower.contains("_vi.") || lower.contains("vi.vtt") || lower.contains("vietnamese") || lower.contains(".dubbed.") {
            "vie"
        } else if lower.contains(".zh.") || lower.contains("_zh.") || lower.contains("zh.vtt") || lower.contains("cn") || lower.contains("chinese") {
            "chi"
        } else if lower.contains(".en.") || lower.contains("_en.") || lower.contains("en.vtt") || lower.contains("english") {
            "eng"
        } else if lower.contains(".ja.") || lower.contains("_ja.") || lower.contains("ja.vtt") || lower.contains("japanese") {
            "jpn"
        } else if lower.contains(".ko.") || lower.contains("_ko.") || lower.contains("ko.vtt") || lower.contains("korean") {
            "kor"
        } else {
            "chi" // Mặc định video gốc nếu không có hậu tố dubbed thường là tiếng Trung
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([750.0, 550.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "Video Merger GUI (Rust)",
        options,
        Box::new(|cc| Ok(Box::new(VideoMergerApp::new(cc)))),
    )
}