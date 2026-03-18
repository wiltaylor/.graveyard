param(
    [Parameter(ParameterSetName='Version')]
    [switch]$Version,
    
    [Parameter(ParameterSetName='Apply')]
    [switch]$Apply,
    
    [Parameter(ParameterSetName='Test')]
    [switch]$Test,

    [Parameter(ParameterSetName='Validate')]
    [switch]$Validate,

    [Parameter(ParameterSetName='Gather')]
    [switch]$Gather,

    [Parameter(ParameterSetName='Clean')]
    [switch]$Clean,

    [Parameter(ParameterSetName='Gather')]
    [Parameter(ParameterSetName='Validate')]
    [Parameter(ParameterSetName='Test')]
    [Parameter(ParameterSetName='Apply')]
    [string]$Script,

    [Parameter(ParameterSetName='Test')]
    [Parameter(ParameterSetName='Apply')]
    [string]$Settings,

    [Parameter(ParameterSetName='Gather')]
    [string]$Output
    
)

$FCEEXE = ".\FCE\FlexibleConfigEngine.exe"
$ZipURL = "https://github.com/wiltaylor/FCE/releases/download/TEST/FCE-0.1.0-unstable.2-Win.zip"

if(!(Test-Path .\FCE) -and $Clean -ne $true)
{
    if(!(Test-Path .\fce.zip))
    {
        Invoke-WebRequest -Uri $ZipURL -OutFile .\fce.zip
    }

    Expand-Archive -Path fce.zip -DestinationPath .\FCE
    Remove-Item .\fce.zip -Force
}

if($Version)
{
    &$FCEEXE version
}

if($Apply)
{
    $para = "apply "

    if($Script -ne "") {$para += "-s $script "}
    if($Settings -ne "") {$para += "-c $Settings "}

    Start-Process $FCEEXE -ArgumentList $para -Wait -NoNewWindow
}

if($Test)
{
    $para = "test "

    if($Script -ne "") {$para += "-s $script "}
    if($Settings -ne "") {$para += "-c $Settings "}

    Start-Process $FCEEXE -ArgumentList $para -Wait -NoNewWindow
}

if($Validate)
{
    $para = "valid "
    if($Script -ne "") {$para += "-s $script "}

    Start-Process $FCEEXE -ArgumentList $para -Wait -NoNewWindow
}

if($Gather)
{
    $para = "gather "

    if($Script -ne "") {$para += "-s $script "}
    if($Settings -ne "") {$para += "-o $Output "}

    Start-Process $FCEEXE -ArgumentList $para -Wait -NoNewWindow
}

if($Clean)
{
    Remove-Item .\FCE -Force -Recurse -ErrorAction SilentlyContinue
    Remove-Item *.log -Force -ErrorAction SilentlyContinue
    Remove-Item .\fce.zip -Force -ErrorAction SilentlyContinue
}