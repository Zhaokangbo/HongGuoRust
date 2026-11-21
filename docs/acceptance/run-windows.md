# 在 Windows 10/11 x64 上运行

从 Windows M1 回归工作流下载 HongGuoRust-M2-windows-x64，将 ZIP 完整解压。该验证包包含正式默认构建的 hongguorust.exe 和运行时部署脚本。需要 Microsoft Edge WebView2 Runtime。打开解压目录中的 PowerShell，执行：

```powershell
.\get-mpv.ps1 -Destination .
.\hongguorust.exe
```

脚本从固定上游地址下载并校验归档和 DLL，使用 7-Zip 或 Windows tar 解压。libmpv-2.dll 必须位于 EXE 旁边。若系统缺少 vulkan-1.dll，优先安装显卡厂商提供的 Vulkan 运行时。构建机也可用随附的 prepare-mpv-msvc.ps1 和 VS C++ Build Tools、Git、CMake 生成固定版本的 loader。点击“打开本地 MP4”，选择文件。播放日志位于 %LOCALAPPDATA%\HongGuoRust\logs。
