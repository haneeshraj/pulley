# Pulley test harness helper (development only).
# Watches the windows of ONE process (an app instance the harness launched) and clicks the
# "Continue" button of After Effects' "start in safe mode?" prompt, which appears after a
# previous test instance was force-closed. Never touches other processes.
param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [int]$Seconds = 240
)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$AE = [System.Windows.Automation.AutomationElement]
$deadline = (Get-Date).AddSeconds($Seconds)
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
            if ($text -notmatch "(?i)safe mode") { continue }
            $buttons = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
            foreach ($b in $buttons) {
                if ($b.Current.Name -match "^(?i)continue") {
                    $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                    "clicked Continue on the safe-mode prompt"
                }
            }
        }
    } catch {}
    Start-Sleep -Milliseconds 1500
}
