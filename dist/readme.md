# Video Merger GUI (Rust Edition)

Ứng dụng đồ họa chuyên nghiệp viết bằng **Rust (eframe/egui)** hỗ trợ đóng gói, gộp video với nhiều track âm thanh và phụ đề đa ngôn ngữ theo chuẩn công nghiệp (ISO 639-2).

---

## 🚀 Các Tính Năng Nổi Bật

1. **Siêu Tốc & Giữ Nguyên Độ Sắc Nét 100%:**
   - Mặc định sử dụng cơ chế **Stream Copy (`-c:v copy`)** giúp gộp video dung lượng hàng gigabyte chỉ trong vài giây đến 1 phút mà không làm giảm dù chỉ 1 điểm ảnh.
2. **Nhận Diện Metadata & Độ Phân Giải Tự Động:**
   - Tự động dùng `ffprobe` trích xuất độ phân giải thực tế (480p, 720p, 1080p, 2K, 4K) và tổng thời lượng video.
3. **Quản Lý Track Thông Minh & Ghi Đè Subtitle Cho File `.mkv`:**
   - Cho phép nạp trực tiếp file `.mkv` đã merge trước đó để cập nhật/thay thế file phụ đề tiếng Việt (`vi.vtt`) mới mà không bị trùng lặp với track cũ.
4. **Tự Động Gán Mã Ngôn Ngữ Chuẩn Quốc Tế (ISO 639-2):**
   - Phân tích hậu tố tên file (`.vi.`, `.zh.`, `.en.`, `.dubbed.`...) để tự động gán nhãn track (`vie`, `chi`, `eng`) tương thích hoàn hảo với VLC, Media Player, TV thông minh.
5. **Tiến Độ Real-time Pipeline 60 FPS:**
   - Báo cáo số giây đã trôi qua, thời điểm xử lý thực tế và thanh % chuyển động êm ái, mượt mà.
   - Hỗ trợ nút **🛑 Dừng Ngay (Cancel)** an toàn giữa chừng bằng Atomic Flag.

---

## 📦 Yêu Cầu Hệ Thống & Cài Đặt

- **Hệ điều hành:** Windows 10 / 11 (64-bit).
- **Yêu cầu phụ thuộc:** Máy tính cần cài đặt sẵn **FFmpeg** và đưa vào biến môi trường `PATH` (hoặc đặt chung thư mục với tệp thực thi).
  - Kiểm tra trong PowerShell: `ffmpeg -version` và `ffprobe -version`.

---

## 🛠 Hướng Dẫn Sử Dụng

1. Chạy tệp **`video_merger_gui.exe`**.
2. **Tab 1: 🎬 Gộp Video & Audio/Sub:**
   - **Bước 1:** Bấm `📁 Chọn Video Gốc` để nạp file `.mp4` hoặc `.mkv`.
   - **Bước 2:** Bấm `🎵 Thêm File Audio` (hỗ trợ chọn nhiều file `.mp3`, `.m4a`, `.aac`, `.mp4`).
   - **Bước 3:** Bấm `💬 Thêm Phụ Đề` (chọn 1 hoặc nhiều file `.vtt`, `.srt`).
   - **Bước 4 (Tùy chọn):** Mở `⚙ Tùy chọn nâng cao` nếu muốn đổi đường dẫn lưu hoặc bật nén lại video.
   - **Bước 5:** Bấm `🚀 Bắt đầu Quá Trình Merge` và theo dõi thanh tiến độ.
3. **Tab 2: 📖 Hướng Dẫn Chi Tiết:**
   - Xem cặn kẽ quy ước đặt tên, giải thích chi tiết về codec và độ phân giải ngay trong app.
