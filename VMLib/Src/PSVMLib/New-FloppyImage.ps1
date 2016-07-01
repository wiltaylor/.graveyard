using namespace VMLib.Disk;

function New-FloppyImage{
	param($Path, $Target)

	$floppybuilder = New-Object VMLib.Disk.FloppyBuilder

	$floppybuilder.CreateDisk($Path, $Target)
}