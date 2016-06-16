$script:HypervisorFactory = New-Object VMLib.HypervisorFactory


foreach($file in Get-ChildItem -Path $PSScriptRoot -File)
{
	. $file.FullName
}