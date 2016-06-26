<# 
.SYNOPSIS
	Creates snapshot of VM.
.DESCRIPTION
	Creates a snapshot of target VM.
.PARAMETER Name
	Name of snapshot.
.PARAMETER Description
	Description of snapshot.
.EXAMPLE
	Saves snapshot on current vm
	$vm | Save-VMSnapshot -Name "MySnapshot"
.OUTPUTS
	IVMSnapshot
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Save-VMSnapshot
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, [string]$Name, [string]$Description = "Snapshot created by VMLib")

	Process 
	{
		$VM.CreateSnapshot($Name, $Description)
	}
}