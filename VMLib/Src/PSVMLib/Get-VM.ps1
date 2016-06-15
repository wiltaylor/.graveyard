<# 
.SYNOPSIS
	Gets a VM object from a vm path.
.DESCRIPTION
	Retrives existing VM. Allowing you to interact with it.
.PARAMETER Path
	Path to existing VM.
.EXAMPLE
	Get instance of my vm
	Get-VM -Path c:\VMs\Myvm.vmx
.OUTPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Get-VM
{
	param([Parameter(Mandatory=$true)][string]$Path)

	Process 
	{

	}
}