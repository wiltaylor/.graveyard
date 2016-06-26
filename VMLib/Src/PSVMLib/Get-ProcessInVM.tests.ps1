.  $PSScriptRoot\Get-ProcessInVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Get-ProcessInVM" {
	Context "Can retrive processes from virtual machine." {
		
		$fakeVM = New-FakeVM -Processes @(
			[pscustomobject]@{
				Name = "Explorer"
				ProcessID = 1
			},
			[pscustomobject]@{
				Name = "Cmd"
				ProcessID = 2
			}
		)
		
		It "Can list all Processes in VM" {
			$result = $fakeVM | Get-ProcessInVM 
			$result[0].Name | should be "Explorer"
			$result[1].Name | should be "Cmd"
		}

		It 'Should only return processes with name specified' {
			$result = $fakeVM | Get-ProcessInVM -Name "Cmd"
			$result[0].Name | should be "Cmd"
		}
	}
}