# Script tự động đóng gói ứng dụng ra thư mục dist
$ErrorActionPreference = "Stop"

Write-Host ">>> [1/3] Đang biên dịch bản Release tối ưu hóa..." -ForegroundColor Cyan
cargo build --release

Write-Host ">>> [2/3] Chuẩn bị thư mục dist/..." -ForegroundColor Cyan
if (!(Test-Path "dist")) {
    New-Item -ItemType Directory -Path "dist" | Out-Null
}

Write-Host ">>> [3/3] Sao chép tệp thực thi và tài liệu..." -ForegroundColor Cyan
Copy-Item "target\release\video_merger_gui.exe" "dist\" -Force

# Sao chép ffmpeg và ffprobe đi kèm nếu có sẵn trong hệ thống
$ffmpegCmd = Get-Command ffmpeg -ErrorAction SilentlyContinue
$ffprobeCmd = Get-Command ffprobe -ErrorAction SilentlyContinue

if ($ffmpegCmd) {
    Copy-Item $ffmpegCmd.Source "dist\" -Force
    Write-Host "    + Đã đính kèm: ffmpeg.exe" -ForegroundColor Green
}
if ($ffprobeCmd) {
    Copy-Item $ffprobeCmd.Source "dist\" -Force
    Write-Host "    + Đã đính kèm: ffprobe.exe" -ForegroundColor Green
}

Write-Host ">>> HOÀN TẤT! Sản phẩm sẵn sàng tại: dist\video_merger_gui.exe" -ForegroundColor Green

