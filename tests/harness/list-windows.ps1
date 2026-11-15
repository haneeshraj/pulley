# Development only: log the windows (and their controls) of ONE process every few seconds.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$Seconds = 120, [string]$Out = "windows.log")
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$AE = [System.Windows.Automation.AutomationElement]
$deadline = (Get-Date).AddSeconds($Seconds)
$pidCond = New-Object System.Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $ProcessId)
while ((Get-Date) -lt $deadline) {
    try {
        $wins = $AE::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $pidCond)
        foreach ($w in $wins) {
            $line = "{0} | {1} | {2}" -f (Get-Date -Format HH:mm:ss), $w.Current.ClassName, $w.Current.Name
            Add-Content -Path $Out -Value $line
            if ($w.Current.Name -match "(?i)save|preset" -or $w.Current.ClassName -eq "#32770") {
                $all = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
                foreach ($e in $all) {
                    Add-Content -Path $Out -Value ("    {0} | {1} | id={2} | {3}" -f $e.Current.ControlType.ProgrammaticName, $e.Current.ClassName, $e.Current.AutomationId, $e.Current.Name)
                }
            }
        }
    } catch {}
    Start-Sleep -Seconds 4
}
