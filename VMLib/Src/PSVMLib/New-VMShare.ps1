<# 
.SYNOPSIS
	Creates a shared folder on VM
.DESCRIPTION
	Creates a shared folder on the VM.
.PARAMETER Name
	Name of share.
.PARAMETER Path
	Path to share on host.
.PARAMETER ReadOnly
	Set this switch to force share to be readonly.
.EXAMPLE
	Share out install folder to vm.
	$vm | New-VMShare -Path c:\installs -Name Installs
.OUTPUTS
	IVMShare
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function New-VMShare
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$VM, [Parameter(Mandatory=$true)][string]$Name, [switch]$ReadOnly, [Parameter(Mandatory=$true)][string]$Path)

	Process 
	{
		$VM.AddSharedFolder($Name, $Path, -not($ReadOnly))
	}
}