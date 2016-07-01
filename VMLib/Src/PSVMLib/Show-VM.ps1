<# 
.SYNOPSIS
	Shows VM in default UI.
.DESCRIPTION
	Shows VM User interface
.EXAMPLE
	Show GUI for vm
	$vm | Show-VM
.INPUTS
	IVirtualMachine object.
#> 
[CmdletBinding]
function Show-VM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM)

	$VM.OpenLocalGUI()
}