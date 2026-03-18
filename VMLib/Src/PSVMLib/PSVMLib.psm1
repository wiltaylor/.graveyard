
$script:HypervisorFactory = [VMLib.Hypervisor.HypervisorFactory]::GetInstance()

foreach($file in Get-ChildItem -Path $PSScriptRoot -File)
{
	if($file.Extension -eq ".ps1"){
		. $file.FullName
	}
}