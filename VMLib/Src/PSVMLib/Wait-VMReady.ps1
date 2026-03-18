<# 
.SYNOPSIS
	Wait for VM to become ready.
.DESCRIPTION
	Wait for VM to become ready or powered off.
.PARAMETER Off
	Set this switch to wait for VM to be powered off or VM will wait till tools are ready.
.EXAMPLE
	Wait for VM to power off.
	$vm, | Wait-VMReady -Off
.EXAMPLE
	Wait for VM to become ready.
	$vm, | Wait-VMReady
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Wait-VMReady {
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
		  [switch]$Off)
		  
		  if($off){
			$vm.WaitTillOff()
		  }else{
			$vm.WaitTillReady()
		  }
}