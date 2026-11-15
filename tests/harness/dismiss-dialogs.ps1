# Pulley test harness helper (development only).
# Watches the windows of ONE process (an app instance the harness launched) and clicks
# "Continue" on After Effects' startup prompts that appear after a previous test instance was
# force-closed: the older "start in safe mode?" message box, and the "Crash Repair Options"
# dialog. The second one is drawn by After Effects itself, so Windows UI Automation can't see
# its text or buttons; it is read with the Windows OCR engine instead, and the button is
# clicked by posting mouse messages to that window. Never touches other processes.
param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [int]$Seconds = 240
)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Runtime.WindowsRuntime
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class HarnessNative {
    public struct RECT { public int Left, Top, Right, Bottom; }
    public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT r);
    [DllImport("user32.dll")] public static extern IntPtr ChildWindowFromPointEx(IntPtr parent, POINT pt, uint flags);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr hwnd, ref POINT pt);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint msg, IntPtr w, IntPtr l);
}
"@
$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
function Await($op, [Type]$t) { $task = $asTask.MakeGenericMethod($t).Invoke($null, @($op)); $task.Wait(-1) | Out-Null; $task.Result }

# OCR words of a window: list of @{ Text; X; Y; W; H } in window coordinates.
function Read-WindowWords([IntPtr]$h) {
    $r = New-Object HarnessNative+RECT
    [void][HarnessNative]::GetWindowRect($h, [ref]$r)
    $w = $r.Right - $r.Left; $hh = $r.Bottom - $r.Top
    if ($w -le 0 -or $hh -le 0) { return @() }
    $bmp = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [void][HarnessNative]::PrintWindow($h, $hdc, 2)
    $g.ReleaseHdc($hdc); $g.Dispose()
    $png = Join-Path $env:TEMP ("pulley-harness-dialog-" + $ProcessId + ".png")
    $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    try {
        $file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($png)) ([Windows.Storage.StorageFile])
        $stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
        $decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
        $sb = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
        $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
        $res = Await ($engine.RecognizeAsync($sb)) ([Windows.Media.Ocr.OcrResult])
        $stream.Dispose()
        $words = @()
        foreach ($line in $res.Lines) { foreach ($x in $line.Words) {
            $words += @{ Text = $x.Text; X = $x.BoundingRect.X; Y = $x.BoundingRect.Y; W = $x.BoundingRect.Width; H = $x.BoundingRect.Height } } }
        return $words
    } finally { Remove-Item $png -ErrorAction SilentlyContinue }
}

# Click a point (window coordinates) by posting mouse messages to the child window under it.
function Click-WindowPoint([IntPtr]$h, [int]$x, [int]$y) {
    $r = New-Object HarnessNative+RECT
    [void][HarnessNative]::GetWindowRect($h, [ref]$r)
    $screen = New-Object HarnessNative+POINT
    $screen.X = $r.Left + $x; $screen.Y = $r.Top + $y
    $target = $h
    $pt = $screen
    for ($depth = 0; $depth -lt 8; $depth++) {
        $local = $pt
        [void][HarnessNative]::ScreenToClient($target, [ref]$local)
        $child = [HarnessNative]::ChildWindowFromPointEx($target, $local, 1 -bor 4)  # skip invisible, transparent
        if ($child -eq [IntPtr]::Zero -or $child -eq $target) { break }
        $target = $child
    }
    $client = $screen
    [void][HarnessNative]::ScreenToClient($target, [ref]$client)
    $lp = [IntPtr](($client.Y -shl 16) -bor ($client.X -band 0xffff))
    [void][HarnessNative]::PostMessage($target, 0x0200, [IntPtr]0, $lp)  # WM_MOUSEMOVE
    [void][HarnessNative]::PostMessage($target, 0x0201, [IntPtr]1, $lp)  # WM_LBUTTONDOWN
    Start-Sleep -Milliseconds 80
    [void][HarnessNative]::PostMessage($target, 0x0202, [IntPtr]0, $lp)  # WM_LBUTTONUP
}

$AE = [System.Windows.Automation.AutomationElement]
$deadline = (Get-Date).AddSeconds($Seconds)
$lastReported = ""
$pidCond = New-Object System.Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $ProcessId)
$btnCond = New-Object System.Windows.Automation.PropertyCondition($AE::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
while ((Get-Date) -lt $deadline) {
    if (-not (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) { exit 0 }
    try {
        $wins = $AE::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $pidCond)
        foreach ($w in $wins) {
            $text = ""
            try {
                $texts = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
                foreach ($t in $texts) { $text += " " + $t.Current.Name }
            } catch {}
            if ($text -match "(?i)safe mode") {
                $buttons = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
                foreach ($b in $buttons) {
                    if ($b.Current.Name -match "^(?i)continue") {
                        $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                        "clicked Continue on the safe-mode prompt"
                    }
                }
                continue
            }
            if ($w.Current.ClassName -ne "#32770") { continue }
            # A dialog drawn by the app itself: read it.
            $h = [IntPtr]$w.Current.NativeWindowHandle
            $words = Read-WindowWords $h
            $all = ($words | ForEach-Object { $_.Text }) -join " "
            if ($all -notmatch "(?i)crash repair") {
                # Any other dialog (a warning or error raised by a test): report its text so the
                # run log shows it, then press OK so the remaining jobs can still run.
                $ok = $words | Where-Object { $_.Text -ceq "OK" } | Select-Object -Last 1
                if ($ok -and $all -ne $lastReported) {
                    "dialog: $all"
                    $lastReported = $all
                    Click-WindowPoint $h ([int]($ok.X + $ok.W / 2)) ([int]($ok.Y + $ok.H / 2))
                    Start-Sleep -Seconds 2
                } elseif ($ok) {
                    Click-WindowPoint $h ([int]($ok.X + $ok.W / 2)) ([int]($ok.Y + $ok.H / 2))
                    Start-Sleep -Seconds 2
                }
                continue
            }
            $btn = $words | Where-Object { $_.Text -match "^(?i)continu" } | Select-Object -Last 1
            if (-not $btn) { continue }
            Click-WindowPoint $h ([int]($btn.X + $btn.W / 2)) ([int]($btn.Y + $btn.H / 2))
            "clicked Continue on the Crash Repair Options dialog"
            Start-Sleep -Seconds 3
        }
    } catch {}
    Start-Sleep -Milliseconds 1500
}
