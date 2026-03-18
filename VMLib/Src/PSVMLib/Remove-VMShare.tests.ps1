.  $PSScriptRoot\Remove-VMShare.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Remove-VMShare" {
	Context "Removes VM share from vm" {
		
		$script:TestFlag = $false
		$fakeVM = New-FakeVM -RemoveSharedFolder {
			param($Name)

			if($Name -eq "MyShare") { $script:TestFlag = $true }
		}
		
		It "Remove share" {
			$fakeVM | Remove-VMShare -Name MyShare
			$script:TestFlag | should be $true
		}
	}
}