using namespace VMLib.Disk;

function New-VMDisk{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$Hypervisor, $Path, [long]$Size)

	$diskbuilder = $Hypervisor.GetDiskBuilder()
	$diskbuilder.CreateDisk($Path, [VMLib.Disk.DiskType]::SCSI, $Size)
}