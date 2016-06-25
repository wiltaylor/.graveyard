<# 
.SYNOPSIS
	Creates new vm hardware object.
.DESCRIPTION
	Creates a new virtual machine hardware item. Used with New-VMInfo cmldet to 
	create the description of a new virtual machine to be created.

	The object this cmdlet returns has a Settings property that can be changed
	for more fine tuned control of the hardware item being added. This will
	included hypervisor specific settings.
.PARAMETER Network
	Create a network card.
.PARAMETER Disk
	Create a disk
.PARAMETER DiskType
	Used to specify the type of disk to create. Valid options are Floppy, HardDisk and CDRom.
.EXAMPLE
	Create a new Network card.
	$nic = New-VMHardware -Network ; $nic.Settings["NetworkType"] = NAT
.EXAMPLE
	Create a HardDisk
	$hd = New-VMHardware -Disk HardDisk
.OUTPUTS
	Returns a IVMHardware object.
#> 
[CmdletBinding]
function New-VMHardware 
{
	param([Parameter(ParameterSetName="Network")][Switch]$Network, [Parameter(ParameterSetName="Disk")][Switch]$Disk, 
	[Parameter(ParameterSetName="Disk", Mandatory = $true, Position = 0)][ValidateSet("Floppy", "HardDisk", "CDRom")]$DiskType)
		
	Process 
	{
		if($Network) { New-Object VMNetwork}

		if($Disk){
			if($DiskType -eq "Floppy") { New-Object VMFloppyDisk}
			if($DiskType -eq "CDRom") { New-Object VMCDrom}
			if($DiskType -eq "HardDisk") { New-Object VMHardDisk}
		}
			
	}
}