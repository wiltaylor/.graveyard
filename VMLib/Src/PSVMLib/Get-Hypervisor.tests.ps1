.  $PSScriptroot\Get-Hypervisor.ps1
.  $PSScriptroot\Fakes\Fake_HypervisorFactory.ps1

Describe "Get-Hypervisor" {
	Context "Calling function will return whatever HypervisorFactory returns." {
		It "Returns what Hypervisor factory returns" {
			$script:HypervisorFactory = New-FakeHypervisorFactory -GetHypervisorNames { @("Hypervisor1", "Hypervisor2")}
			
			$results = Get-Hypervisor

			$results[0] | should be "Hypervisor1"
			$results[1] | should be "Hypervisor2"
 		}
	}
}