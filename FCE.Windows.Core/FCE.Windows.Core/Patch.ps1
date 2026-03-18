param($ignorePatches = @("*language pack*", "*Windows Defender*"), [switch]$Test)

$script:Session = New-Object -ComObject Microsoft.Update.Session

<#
    .SYNOPSIS
    Uses windows update API to check what patches need to be installed.
    .PARAMETER Skip
    Array of patches to skip. Uses powershell -Like syntax.
#>
function Get-AvailableUpdates
{
    param([Parameter(Mandatory = $true)]
        [string[]]$skip)

    $Searcher = $script:Session.CreateUpdateSearcher()
    $UpdateServiceManager = New-Object -ComObject Microsoft.Update.ServiceManager
    $Query = "IsInstalled=0 and Type='Software' and IsHidden=0"
    $Patches = $Searcher.Search($Query).Updates
    $ReturnPatches = New-Object -ComObject Microsoft.Update.UpdateColl

    if ($patches -eq $null -or $patches.Count -eq 0 ) 
    { 
        return $null
    }

    foreach ($patch in $Patches)
    {
        $shouldSkip = $false
        foreach ($banned in $skip)
        {
            if ($patch.title.ToLower() -like $banned) { $shouldSkip = $true }
        }

        if (-not $shouldSkip) { $ReturnPatches.Add($patch) | out-null }
    }

    Write-Output $ReturnPatches
}

<#
    .SYNOPSIS
    Downloads patches to the windows install cache ready to install.
    
    .PARAMETER Patches
    Patch collection of patches to download. Use Get-AvailableUpdates to get the list.
#>
function Get-PatchCache
{
    param($Patches)

    $patchcount = $patches.Count
    $patchindex = 0

    foreach ($patch in $patches)
    {
        $patchindex++
        if ($patch.IsDownloaded)
        {
            Write-Host "Patch [$patchindex/$patchcount] $($patch.Title) is already downloaded!"
        }
        else
        {
            Write-Host "Patch [$patchindex/$patchcount] $($patch.Title) is being downloaded."
            $currentupdate = New-Object -ComObject Microsoft.Update.UpdateColl
            $currentupdate.Add($patch) | Out-Null
            $downloader = $script:Session.CreateUpdateDownloader() 
            $downloader.Updates = $currentupdate
            $downloader.Download() | Out-Null
        }
    }
}

<#
    .SYNOPSIS
    Accepts the EULA for all patches passed in.
    .PARAMETER Patches
    Patch collection of patches to approve eula on. Use Get-AvailableUpdates to get this list.
#>
function Approve-PatchEULA
{
    param($Patches)

    foreach ($patch in $Patches)
    {
        if (-not $patch.EulaAccepted)
        {
            $patch.AcceptEula() | Out-Null
        }
    }
}

<#
    .SYNOPSIS
    Installs all patches passed in.
    .PARAMETER Patches
    Patch collection of patches to install. Use Get-AvailableUpdates to get this list.

#>
function Install-Updates()
{
    param($Patches)

    $installer = $script:Session.CreateUpdateInstaller()

    $patchcount = $Patches.Count
    $patchindex = 0

    Write-Host "Waiting for patch installer to be ready..."
    if ($installer.IsBusy)
    {
        foreach ($i in 1..20) { if ($installer.IsBusy) { Start-Sleep -Seconds 5 } else { break }}

        if ($installer.IsBusy)
        {
            Write-Host "Patch installer still not ready...rebooting..."
            Exit 3010
        }
    }

    if ($installer.RebootRequiredBeforeInstallation) 
    { 
        Write-Host "Pending reboot detected...rebooting..."
        Exit 3010
    }

    foreach ($patch in $Patches)
    {
        $patchindex++

        $currentupdate = New-Object -ComObject Microsoft.Update.UpdateColl
        $currentupdate.Add($patch) | Out-Null

        Write-Host "Installing [$patchindex/$patchcount] $($patch.Title)"
        try 
        {                
            $installer.AllowSourcePrompts = $false
            $installer.IsForced = $true
            $installer.Updates = $currentupdate
            $installer.install() | Out-Null
            Write-Host "Installed Ok"
        }
        catch
        {
            Write-Host "Failed to install $($patch.Title) - Will try again next reboot if still applicable."
        }
    }
}
Write-Host "Searching for updates..."

$Patches = Get-AvailableUpdates -skip @($ignorePatches) 

if ($Patches -eq $null -or $Patches.Count -eq 0)
{
    Write-Host "No more updates...exiting..."
    Write-Host "###PATCH FINISHED###"
}

if($Test) 
{ 
    Write-Host "###MORE PATCHS###"
    exit 500
}

Write-Host "Downloading patches..."
Get-PatchCache -Patches $Patches
Approve-PatchEULA -Patches $Patches
Write-Host "Installing Patches..."
Install-Updates -Patches $Patches

Write-Host "Current round of patches installed...Requires reboot"
Write-Host "###REBOOT###"
exit 3010