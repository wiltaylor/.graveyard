.  $PSScriptRoot\Get-VM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1
.  $PSScriptroot\Fakes\Fake_Hypervisor.ps1

Describe "Get-VM" {
	Context "Can retrive VM from Hypervisor" {
		$FakeHypervisor = New-FakeHypervisor -OpenVM {
			param($Path)

			if($Path -eq "c:\Expected.vmx") {
				 New-FakeVM -State "FakeState"
			}
		}		
	
		It "Calling expected Path returns object" {
			$result = $FakeHypervisor | Get-VM -Path "c:\Expected.vmx"
			$result.State | should be "FakeState"
		}
	}
}