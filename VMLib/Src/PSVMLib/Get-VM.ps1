<# 
.SYNOPSIS
	Gets a VM object from a vm path.
.DESCRIPTION
	Retrives existing VM. Allowing you to interact with it.
.PARAMETER Path
	Path to existing VM.
.PARAMETER Hypervisor
	Hypervisor object to use to retrive the VM.
.EXAMPLE
	Get instance of my vm
	Get-VM -Path c:\VMs\Myvm.vmx
.INPUTS
	Hypervisor object.
.OUTPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Get-VM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$Hypervisor,[Parameter(Mandatory=$true, ParameterSetName="VM")][string]$Path,[Parameter(Mandatory=$true, ParameterSetName="AllVM")][Switch]$AllRunning)

	Process 
	{
		if($AllRunning) {
			foreach($vm in $Hypervisor.GetAllRunningVM()) { $vm }
		} else {
			$Hypervisor.OpenVM((Convert-Path $Path))
		}
	}
}