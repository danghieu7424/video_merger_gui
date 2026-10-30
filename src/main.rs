use eframe::egui;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;
use rfd::FileDialog;

#[derive(Clone, Debug)]
pub enum MergeProgressMsg {
    Progress {
        percent: f32,          // 0.0 -> 1.0
        elapsed_secs: u64,     // Thời gian đã chạy (giây)
        current_time_secs: u64,// Thời điểm đã xử lý xong trong video
        total_secs: u64,       // Tổng thời lượng video
    },
    Success(String),
    Error(String),
}

struct VideoMergerApp {
    video_path: String,
    audio_paths: Vec<String>,
    sub_paths: Vec<String>,
    output_path: String,
    
    video_resolution: String, // Độ phân giải tự động nhận diện (vd: "720p", "1080p")
    total_duration_secs: f64, // Tổng thời lượng video gốc tính bằng giây

    reencode_video: bool,
    video_codec: String,
    status_message: String,
    is_processing: bool,

    // Dữ liệu tiến độ Pipeline hiển thị trên GUI
    progress_ratio: f32,
    progress_line: String,
    detail_log: String,

    // Kênh truyền tin (channel) giữa background worker và UI thread
    rx: Receiver<MergeProgressMsg>,
    tx: Sender<MergeProgressMsg>,
}

impl Default for VideoMergerApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            video_path: String::new(),
            audio_paths: Vec::new(),
            sub_paths: Vec::new(),
            output_path: String::from("output.mkv"),
            video_resolution: String::new(),
            total_duration_secs: 0.0,
            reencode_video: false,
            video_codec: String::from("copy"),
            status_message: String::from("Sẵn sàng. Vui lòng chọn các file đầu vào."),
            is_processing: false,
            progress_ratio: 0.0,
            progress_line: String::new(),
            detail_log: String::new(),
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
        // Kiểm tra thông điệp tiến độ từ background thread gửi về
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                MergeProgressMsg::Progress {
                    percent,
                    elapsed_secs,
                    current_time_secs,
                    total_secs,
                } => {
                    self.progress_ratio = percent;
                    let pct_int = (percent * 100.0) as u32;
                    self.progress_line = format!(
                        "Pipeline: [{pct_int}/100 % ({elapsed_secs}s)] - Đã xử lý {current_time_secs}s / {total_secs}s"
                    );
                    ctx.request_repaint(); // Yêu cầu vẽ lại UI mượt mà theo từng frame tiến độ
                }
                MergeProgressMsg::Success(msg) => {
                    self.is_processing = false;
                    self.progress_ratio = 1.0;
                    self.status_message = String::from("Hoàn tất!");
                    self.detail_log = format!("⤷ {}", msg);
                    ctx.request_repaint();
                }
                MergeProgressMsg::Error(err) => {
                    self.is_processing = false;
                    self.status_message = format!("❌ Lỗi: {}", err);
                    self.detail_log = format!("⤷ Quá trình dừng lại do lỗi.");
                    ctx.request_repaint();
                }
            }
        }

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        ctx.set_style(style);

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui.add_space(5.0);
                ui.heading("🎬 Trình Gộp Video & Phụ Đề Đa Ngôn Ngữ (Rust)");
                ui.separator();
                ui.add_space(5.0);

                // --- 1. CHỌN VIDEO GỐC ---
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.is_processing, egui::Button::new("📁 Chọn Video Gốc (.mp4)")).clicked() {
                        if let Some(path) = FileDialog::new().pick_file() {
                            let path_str = path.display().to_string();
                            self.video_path = path_str.clone();

                            // Tự động phân tích Metadata (Độ phân giải & Thời lượng)
                            let (res, duration) = Self::probe_video_metadata(&path_str);
                            self.video_resolution = res;
                            self.total_duration_secs = duration;

                            // Tự động gán đường dẫn file xuất ra cùng thư mục, cùng tên gốc với phần mở rộng .mkv
                            let mut out = path.clone();
                            out.set_extension("mkv");
                            self.output_path = out.display().to_string();
                        }
                    }
                    let display_text = if self.video_path.is_empty() {
                        "Chưa chọn file nào".to_string()
                    } else {
                        format!("{} [Độ phân giải: {}]", self.video_path, self.video_resolution)
                    };
                    ui.add(egui::Label::new(display_text).truncate());
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
                        ui.add(egui::Label::new(format!("• Track bổ sung #{0} [Mã ISO: {1}]: {2}", i + 1, lang.to_uppercase(), audio)).truncate());
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
                        ui.add(egui::Label::new(format!("• Phụ đề #{0} [Mã ISO: {1}]: {2}", i + 1, lang.to_uppercase(), sub)).truncate());
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
                        ui.add(egui::TextEdit::singleline(&mut self.output_path).desired_width(f32::INFINITY));
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
                let is_input_ready = !self.video_path.is_empty() && (!self.audio_paths.is_empty() || !self.sub_paths.is_empty());
                let can_start = is_input_ready && !self.is_processing;

                let button_text = if self.is_processing {
                    "⏳ Đang gộp file (Vui lòng đợi)..."
                } else {
                    "🚀 Bắt đầu Quá Trình Merge"
                };

                if ui.add_enabled(can_start, egui::Button::new(button_text)).clicked() {
                    self.is_processing = true;
                    self.progress_ratio = 0.0;
                    self.progress_line = String::from("Pipeline: Khởi động...");
                    self.detail_log = String::new();
                    self.status_message = String::from("Đang chạy FFmpeg ngầm trong hệ thống...");

                    let tx = self.tx.clone();
                    let video_path = self.video_path.clone();
                    let audio_paths = self.audio_paths.clone();
                    let sub_paths = self.sub_paths.clone();
                    let video_codec = self.video_codec.clone();
                    let output_path = self.output_path.clone();
                    let total_duration = self.total_duration_secs;

                    // Spawning background worker thread
                    std::thread::spawn(move || {
                        Self::execute_ffmpeg_merge(
                            &video_path,
                            &audio_paths,
                            &sub_paths,
                            &video_codec,
                            &output_path,
                            total_duration,
                            &tx,
                        );
                    });
                }

                ui.add_space(5.0);

                // --- 6. KHỐI PIPELINE PROGRESS REAL-TIME ---
                ui.group(|ui| {
                    let progress_bar = egui::ProgressBar::new(self.progress_ratio)
                        .show_percentage()
                        .animate(self.is_processing);
                    ui.add(progress_bar);

                    if !self.progress_line.is_empty() {
                        ui.monospace(&self.progress_line);
                    }
                    if !self.detail_log.is_empty() {
                        ui.label(egui::RichText::new(&self.detail_log).color(egui::Color32::from_rgb(100, 220, 100)));
                    }
                    ui.label(format!("Trạng thái: {}", self.status_message));
                });
            });
        });
    }
}

/****
 * Thực thi lệnh FFmpeg trong luồng Worker độc lập để tránh đóng băng (freeze/crash) giao diện GUI.
 * Đầu vào: Đường dẫn video, danh sách audio, danh sách subtitle, codec, output path.
 * Đầu ra: Result<String, String> báo cáo kết quả chi tiết cho UI.
 ****/
/****
 * Thực thi lệnh FFmpeg trong luồng Worker độc lập để tránh đóng băng giao diện GUI,
 * đồng thời đọc stream stdout (-progress pipe:1) để bắn dữ liệu tiến độ thời gian thực về UI.
 * Đầu vào: Đường dẫn video, danh sách audio, danh sách subtitle, codec, output path, tổng thời lượng video.
 * Đầu ra: Bắn liên tục MergeProgressMsg qua kênh sender tx.
 ****/
impl VideoMergerApp {
    fn execute_ffmpeg_merge(
        video_path: &str,
        audio_paths: &[String],
        sub_paths: &[String],
        video_codec: &str,
        output_path: &str,
        total_duration: f64,
        tx: &Sender<MergeProgressMsg>,
    ) {
        let mut cmd = Command::new("ffmpeg");

        // Ghi đè file nếu đã tồn tại (-y) tránh việc ffmpeg dừng đợi prompt stdin
        cmd.arg("-y");

        // Bật chế độ báo cáo tiến độ qua stdout tiêu chuẩn
        cmd.arg("-progress").arg("pipe:1");

        // 1. Khai báo lần lượt các input file vào FFmpeg
        cmd.arg("-i").arg(video_path);

        for audio in audio_paths {
            cmd.arg("-i").arg(audio);
        }

        for sub in sub_paths {
            cmd.arg("-i").arg(sub);
        }

        // 2. Map Stream tự động chính xác theo thứ tự input
        cmd.arg("-map").arg("0:v:0");

        let total_audio_inputs = 1 + audio_paths.len();
        for i in 0..total_audio_inputs {
            cmd.arg("-map").arg(format!("{}:a:0", i));
        }

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

        // Chuyển hướng stdout để đọc realtime progress
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start_time = Instant::now();
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(MergeProgressMsg::Error(format!("Không thể khởi chạy FFmpeg: {}", e)));
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                // FFmpeg trả về dạng: out_time_ms=12345000 hoặc out_time_us=...
                if line.starts_with("out_time_us=") || line.starts_with("out_time_ms=") {
                    let parts: Vec<&str> = line.split('=').collect();
                    if parts.len() == 2 {
                        if let Ok(time_val) = parts[1].trim().parse::<u64>() {
                            let current_secs = if line.starts_with("out_time_us=") {
                                time_val / 1_000_000
                            } else {
                                time_val / 1_000
                            };

                            let elapsed_secs = start_time.elapsed().as_secs();
                            let percent = if total_duration > 0.0 {
                                ((current_secs as f64 / total_duration) as f32).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };

                            let _ = tx.send(MergeProgressMsg::Progress {
                                percent,
                                elapsed_secs,
                                current_time_secs: current_secs,
                                total_secs: total_duration as u64,
                            });
                        }
                    }
                }
            }
        }

        let status = match child.wait() {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(MergeProgressMsg::Error(format!("Lỗi chờ tiến trình: {}", e)));
                return;
            }
        };

        if status.success() {
            let _ = tx.send(MergeProgressMsg::Success(format!("Toàn bộ video đã xuất thành công -> {}", output_path)));
        } else {
            let _ = tx.send(MergeProgressMsg::Error(String::from("FFmpeg gặp sự cố khi xử lý dữ liệu stream.")));
        }
    }

    // Trích xuất metadata độ phân giải và thời lượng bằng ffprobe
    fn probe_video_metadata(path: &str) -> (String, f64) {
        // 1. Lấy chiều cao (height) để xác định 720p / 1080p / 4K
        let height_res = Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=height",
                "-of", "csv=p=0",
                path,
            ])
            .output();

        let resolution = match height_res {
            Ok(out) => {
                let h_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(h) = h_str.parse::<u32>() {
                    format!("{}p", h)
                } else {
                    String::from("Unknown")
                }
            }
            Err(_) => String::from("Unknown"),
        };

        // 2. Lấy thời lượng (duration tính theo giây)
        let dur_res = Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-show_entries", "format=duration",
                "-of", "csv=p=0",
                path,
            ])
            .output();

        let duration = match dur_res {
            Ok(out) => {
                let d_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                d_str.parse::<f64>().unwrap_or(0.0)
            }
            Err(_) => 0.0,
        };

        (resolution, duration)
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
            .with_inner_size([880.0, 720.0])
            .with_min_inner_size([650.0, 500.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "Video Merger GUI (Rust)",
        options,
        Box::new(|cc| Ok(Box::new(VideoMergerApp::new(cc)))),
    )
}