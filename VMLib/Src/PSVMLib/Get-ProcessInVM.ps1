<# 
.SYNOPSIS
	Gets a name of processes running in VM.
.DESCRIPTION
	Gets the details of the processes running in VM.
.PARAMETER Name
	Name of process to find. If blank all processes will be returned.
.EXAMPLE
	Get all process from vm.
	$processes = $vm | Get-ProcessInVM
.INPUTS
	IVMSnapshot
.OUTPUTS
	IVMProcess
#> 
[CmdletBinding]
function Get-ProcessInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [string]$Name)

	Process 
	{
		foreach($p in $vm.Processes) { 
			if([string]::IsNullOrEmpty($Name)) { 
				$p 
			} else{
				if($Name.ToLower() -eq $p.Name.ToLower()) { $p }
			}
		}
	}
}