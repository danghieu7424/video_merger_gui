use eframe::egui;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use rfd::FileDialog;

#[derive(PartialEq, Clone, Copy)]
enum AppTab {
    Merger,
    Guide,
}

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
    current_tab: AppTab,
    video_path: String,
    audio_paths: Vec<String>,
    sub_paths: Vec<String>,
    output_path: String,
    
    video_resolution: String, // Độ phân giải tự động nhận diện (vd: "720p", "1080p")
    total_duration_secs: f64, // Tổng thời lượng video gốc tính bằng giây
    has_existing_subs: bool,  // File gốc đã có sẵn sub hay chưa
    replace_existing_subs: bool, // Tùy chọn: Loại bỏ sub cũ để thay bằng sub mới

    reencode_video: bool,
    video_codec: String,
    status_message: String,
    is_processing: bool,

    // Cờ báo hủy tác vụ khẩn cấp (Cancel flag)
    cancel_flag: Arc<AtomicBool>,

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
            current_tab: AppTab::Merger,
            video_path: String::new(),
            audio_paths: Vec::new(),
            sub_paths: Vec::new(),
            output_path: String::from("output.mkv"),
            video_resolution: String::new(),
            total_duration_secs: 0.0,
            has_existing_subs: false,
            replace_existing_subs: true,
            reencode_video: false,
            video_codec: String::from("copy"),
            status_message: String::from("Sẵn sàng. Vui lòng chọn các file đầu vào."),
            is_processing: false,
            cancel_flag: Arc::new(AtomicBool::new(false)),
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
        // --- NẠP FONT TIẾNG VIỆT TỪ HỆ THỐNG WINDOWS (HỖ TRỢ CẢ PROPORTIONAL & MONOSPACE) ---
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
            fonts.font_data.insert(
                "segoe_ui".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            // Gán cho Proportional (chữ thường, UI)
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(0, "segoe_ui".to_owned());
            }
            // Gán cho Monospace để chữ "Đã xử lý..." trong progress không bị ô vuông lỗi font
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.insert(0, "segoe_ui".to_owned());
            }
        }
        cc.egui_ctx.set_fonts(fonts);
        Self::default()
    }
}

impl eframe::App for VideoMergerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Kiểm tra thông điệp tiến độ từ background thread gửi về
        let mut should_repaint = false;
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
                    should_repaint = true;
                }
                MergeProgressMsg::Success(msg) => {
                    self.is_processing = false;
                    self.progress_ratio = 1.0;
                    self.status_message = String::from("Hoàn tất!");
                    self.detail_log = format!("⤷ {}", msg);
                    should_repaint = true;
                }
                MergeProgressMsg::Error(err) => {
                    self.is_processing = false;
                    self.status_message = format!("❌ {}", err);
                    self.detail_log = format!("⤷ Quá trình dừng lại.");
                    should_repaint = true;
                }
            }
        }

        if should_repaint {
            ctx.request_repaint();
        }

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        ctx.set_style(style);

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.current_tab, AppTab::Merger, "🎬 Gộp Video & Audio/Sub");
                    ui.selectable_value(&mut self.current_tab, AppTab::Guide, "📖 Hướng Dẫn Chi Tiết");
                });
                ui.separator();
                ui.add_space(5.0);

                match self.current_tab {
                    AppTab::Merger => {
                        self.render_merger_tab(ui);
                    }
                    AppTab::Guide => {
                        self.render_guide_tab(ui);
                    }
                }
            });
        });
    }
}

impl VideoMergerApp {
    // --- TAB 1: GIAO DIỆN CHÍNH THỰC THI GỘP VIDEO ---
    fn render_merger_tab(&mut self, ui: &mut egui::Ui) {
        // --- 1. CHỌN VIDEO GỐC ---
        ui.group(|ui| {
            ui.label(egui::RichText::new("1. Video Gốc (Nền tảng hình ảnh & âm thanh gốc)").strong());
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.is_processing, egui::Button::new("📁 Chọn Video Gốc (.mp4 / .mkv)")).clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Video Files (*.mp4, *.mkv, *.mov, *.webm)", &["mp4", "mkv", "mov", "webm"])
                        .pick_file() 
                    {
                        let path_str = path.display().to_string();
                        self.video_path = path_str.clone();

                        let (res, duration, has_subs) = Self::probe_video_metadata(&path_str);
                        self.video_resolution = res;
                        self.total_duration_secs = duration;
                        self.has_existing_subs = has_subs;

                        let mut out = path.clone();
                        out.set_extension("mkv");
                        self.output_path = out.display().to_string();
                    }
                }
                let display_text = if self.video_path.is_empty() {
                    "Chưa chọn file nào".to_string()
                } else {
                    let sub_info = if self.has_existing_subs { " | Đã có sẵn Subtitle" } else { "" };
                    format!("{} [Độ phân giải: {}{}]", self.video_path, self.video_resolution, sub_info)
                };
                ui.add(egui::Label::new(display_text).truncate());
            });

            if self.has_existing_subs {
                ui.checkbox(
                    &mut self.replace_existing_subs,
                    "🔄 Loại bỏ toàn bộ phụ đề cũ trong file gốc và thay bằng phụ đề mới chọn bên dưới"
                );
            }
        });

        ui.add_space(5.0);

        // --- 2. CHỌN ÂM THANH ---
        ui.group(|ui| {
            ui.label(egui::RichText::new("2. Audio Bổ Sung (Lồng tiếng / Thuyết minh)").strong());
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
        });

        ui.add_space(5.0);

        // --- 3. CHỌN PHỤ ĐỀ ---
        ui.group(|ui| {
            ui.label(egui::RichText::new("3. Phụ Đề Rời (VTT / SRT đa ngôn ngữ)").strong());
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
        });

        ui.add_space(5.0);

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

        ui.add_space(10.0);

        // --- 5. NÚT THỰC THI (DISABLE KHI CHƯA ĐỦ ĐIỀU KIỆN HOẶC ĐANG XỬ LÝ) ---
        let is_input_ready = !self.video_path.is_empty() && (!self.audio_paths.is_empty() || !self.sub_paths.is_empty());
        let can_start = is_input_ready && !self.is_processing;

        ui.horizontal(|ui| {
            let button_text = if self.is_processing {
                "⏳ Đang gộp file (Vui lòng đợi)..."
            } else {
                "🚀 Bắt đầu Quá Trình Merge"
            };

            if ui.add_enabled(can_start, egui::Button::new(egui::RichText::new(button_text).heading())).clicked() {
                self.is_processing = true;
                self.progress_ratio = 0.0;
                self.progress_line = String::from("Pipeline: Khởi động...");
                self.detail_log = String::new();
                self.status_message = String::from("Đang chạy FFmpeg ngầm trong hệ thống...");
                self.cancel_flag.store(false, Ordering::Relaxed);

                let tx = self.tx.clone();
                let video_path = self.video_path.clone();
                let audio_paths = self.audio_paths.clone();
                let sub_paths = self.sub_paths.clone();
                let video_codec = self.video_codec.clone();
                let output_path = self.output_path.clone();
                let total_duration = self.total_duration_secs;
                let replace_existing_subs = self.has_existing_subs && self.replace_existing_subs;
                let cancel_flag = self.cancel_flag.clone();

                std::thread::spawn(move || {
                    Self::execute_ffmpeg_merge(
                        &video_path,
                        &audio_paths,
                        &sub_paths,
                        &video_codec,
                        &output_path,
                        total_duration,
                        replace_existing_subs,
                        cancel_flag,
                        &tx,
                    );
                });
            }

            // Nút Hủy Tiến Trình nếu người dùng muốn dừng giữa chừng
            if self.is_processing {
                if ui.button(egui::RichText::new("🛑 Dừng Ngay").heading().color(egui::Color32::from_rgb(255, 90, 90))).clicked() {
                    self.cancel_flag.store(true, Ordering::Relaxed);
                    self.status_message = String::from("Đang gửi lệnh dừng FFmpeg...");
                }
            }
        });

        ui.add_space(5.0);

        // --- 6. KHỐI PIPELINE PROGRESS REAL-TIME ---
        ui.group(|ui| {
            let progress_bar = egui::ProgressBar::new(self.progress_ratio)
                .show_percentage();
            ui.add(progress_bar);

            if !self.progress_line.is_empty() {
                ui.label(egui::RichText::new(&self.progress_line).strong());
            }
            if !self.detail_log.is_empty() {
                ui.label(egui::RichText::new(&self.detail_log).color(egui::Color32::from_rgb(100, 220, 100)));
            }
            ui.label(format!("Trạng thái: {}", self.status_message));
        });
    }

    // --- TAB 2: HƯỚNG DẪN CHI TIẾT SỬ DỤNG APP ---
    fn render_guide_tab(&self, ui: &mut egui::Ui) {
        ui.heading("📖 Hướng Dẫn Toàn Diện Về Ứng Dụng Video Merger");
        ui.separator();
        ui.add_space(5.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("1. Mục Đích & Nguyên Lý Hoạt Động").strong());
            ui.label("• Ứng dụng này sinh ra để đóng gói video nguyên gốc (.mp4/.mkv) kết hợp cùng nhiều track âm thanh (lồng tiếng/thuyết minh) và nhiều phụ đề rời (.vtt/.srt).");
            ui.label("• Toàn bộ quá trình sử dụng cơ chế Stream Copy (-c:v copy), nghĩa là không mã hóa lại hình ảnh -> Tốc độ xuất cực nhanh (vài giây), giữ nguyên 100% độ sắc nét ban đầu.");
        });

        ui.add_space(5.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("2. Quy Ước Đặt Tên & Nhận Diện Ngôn Ngữ Tự Động").strong());
            ui.label("• Hệ thống tự động phân tích tên file để gán mã ISO 639-2 tương ứng cho các player (VLC, Media Player, TV):");
            ui.label("  - Tên chứa: '.vi.', '_vi.', 'vi.vtt', '.dubbed.', 'vietnamese' -> Nhận diện là: Tiếng Việt (vie)");
            ui.label("  - Tên chứa: '.zh.', '_zh.', 'zh.vtt', 'cn', 'chinese' -> Nhận diện là: Tiếng Trung (chi)");
            ui.label("  - Tên chứa: '.en.', '_en.', 'en.vtt', 'english' -> Nhận diện là: Tiếng Anh (eng)");
            ui.label("  - Video gốc không có hậu tố .dubbed. mặc định được nhận diện là Tiếng Trung (chi).");
        });

        ui.add_space(5.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("3. Cập Nhật Phụ Đề Cho File .MKV Đã Tạo Trước Đó").strong());
            ui.label("• Nếu bạn đã có sẵn 1 file .mkv hoàn chỉnh, nhưng sau đó chỉnh sửa lại nội dung file vi.vtt:");
            ui.label("  1. Tại mục 'Video Gốc', bạn chọn trực tiếp file .mkv đó.");
            ui.label("  2. Hệ thống sẽ phát hiện file đã có sub và hiện ô checkbox 'Loại bỏ toàn bộ phụ đề cũ...'.");
            ui.label("  3. Thêm file vi.vtt mới vào mục phụ đề -> Bấm Merge.");
            ui.label("  -> Kết quả: File .mkv mới sẽ thay thế phụ đề cũ bằng phụ đề mới mà không phải làm lại từ đầu.");
        });

        ui.add_space(5.0);

        ui.group(|ui| {
            ui.label(egui::RichText::new("4. Về Độ Phân Giải & Chất Lượng Video").strong());
            ui.label("• Mỗi file video (.mkv, .mp4) trên thực tế chỉ mang đúng 1 độ phân giải cố định (480p, 720p, 1080p, 2K, 4K).");
            ui.label("• Hệ thống dùng ffprobe tự động trích xuất độ phân giải thực tế của file và hiển thị lên màn hình.");
            ui.label("• Khi nào cần tích 'Encode lại video'?");
            ui.label("  - KHÔNG TÍCH (Khuyên dùng): Xuất video siêu tốc, giữ 100% chất lượng gốc.");
            ui.label("  - TÍCH: Chỉ dùng khi muốn nén ép giảm dung lượng bằng các codec như libx264, libx265.");
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
    // Hàm tìm đường dẫn binary thực thi (ưu tiên file nằm cùng thư mục exe, nếu không có thì tìm trong PATH hệ thống)
    fn get_binary_path(binary_name: &str) -> std::path::PathBuf {
        let exe_name = if cfg!(windows) {
            format!("{}.exe", binary_name)
        } else {
            binary_name.to_string()
        };

        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                let local_bin = parent.join(&exe_name);
                if local_bin.exists() {
                    return local_bin;
                }
            }
        }
        std::path::PathBuf::from(binary_name)
    }

    fn execute_ffmpeg_merge(
        video_path: &str,
        audio_paths: &[String],
        sub_paths: &[String],
        video_codec: &str,
        output_path: &str,
        total_duration: f64,
        replace_existing_subs: bool,
        cancel_flag: Arc<AtomicBool>,
        tx: &Sender<MergeProgressMsg>,
    ) {
        let ffmpeg_bin = Self::get_binary_path("ffmpeg");
        let mut cmd = Command::new(ffmpeg_bin);

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

        // Logic xử lý Subtitle:
        // Nếu file gốc đã có sub và người dùng KHÔNG muốn thay thế (giữ lại sub cũ): map cả sub từ file gốc 0:s
        if !replace_existing_subs {
            cmd.arg("-map").arg("0:s?");
        }

        // Map lần lượt các file sub mới chọn bên dưới
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

        // 6. Gán Metadata ngôn ngữ động cho Subtitle streams mới (index từ 0 trở đi)
        for (i, sub_path) in sub_paths.iter().enumerate() {
            let lang = Self::detect_language_from_filename(sub_path);
            cmd.arg(format!("-metadata:s:s:{}", i)).arg(format!("language={}", lang));
        }

        cmd.arg(output_path);

        // Chuyển hướng stdout để đọc realtime progress
        cmd.stdout(Stdio::piped());
        // Không pipe stderr nếu không đọc để tránh tràn buffer hệ điều hành làm đứng FFmpeg
        cmd.stderr(Stdio::null());

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
            let mut last_emit = Instant::now();

            for line in reader.lines().flatten() {
                // Kiểm tra nếu người dùng nhấn nút Hủy khẩn cấp
                if cancel_flag.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = tx.send(MergeProgressMsg::Error(String::from("Đã hủy quá trình theo yêu cầu của bạn.")));
                    return;
                }

                // CHỈ parse out_time_us (microsecond) để đảm bảo độ chính xác tuyệt đối
                if line.starts_with("out_time_us=") {
                    let parts: Vec<&str> = line.split('=').collect();
                    if parts.len() == 2 {
                        if let Ok(time_us) = parts[1].trim().parse::<u64>() {
                            // Giới hạn tần số gửi tin: 10 tin/giây (~100ms) để UI luôn nhẹ êm 60 FPS
                            if last_emit.elapsed().as_millis() >= 100 {
                                last_emit = Instant::now();
                                let current_secs = time_us / 1_000_000;
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

    // Trích xuất metadata độ phân giải, thời lượng và kiểm tra sub có sẵn bằng ffprobe
    fn probe_video_metadata(path: &str) -> (String, f64, bool) {
        let ffprobe_bin = Self::get_binary_path("ffprobe");

        // 1. Lấy chiều cao (height) để xác định 720p / 1080p / 4K
        let height_res = Command::new(&ffprobe_bin)
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
        let dur_res = Command::new(&ffprobe_bin)
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

        // 3. Kiểm tra file gốc có chứa subtitle stream nào không
        let sub_res = Command::new(&ffprobe_bin)
            .args(&[
                "-v", "error",
                "-select_streams", "s",
                "-show_entries", "stream=index",
                "-of", "csv=p=0",
                path,
            ])
            .output();

        let has_subs = match sub_res {
            Ok(out) => !String::from_utf8_lossy(&out.stdout).trim().is_empty(),
            Err(_) => false,
        };

        (resolution, duration, has_subs)
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