<# 
.SYNOPSIS
	Creates new vm hardware object.
.DESCRIPTION
	Creates a new virtual machine hardware item. Used with New-VMInfo cmldet to 
	create the description of a new virtual machine to be created.

	The object this cmdlet returns has a Settings property that can be changed
	for more fine tuned control of the hardware item being added. This will
	included hypervisor specific settings.
.PARAMETER Type
	Type of hardware to create. Available types are Network, Disk, Sound or Misc
.EXAMPLE
	Create a new Network card.
	$nic = New-VMHardware -Type Network ; $nic.Settings["NetworkType"] = NAT
.OUTPUTS
	Returns a IVMHardware object.
#> 
[CmdletBinding]
function New-VMHardware 
{
	param([Parameter(Mandatory=$true)][ValidateSet('Network', 'Disk', 'Sound', 'Misc')][string]$Type )

	Process 
	{

	}
}