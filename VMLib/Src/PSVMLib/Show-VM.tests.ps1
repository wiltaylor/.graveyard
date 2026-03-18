.  $PSScriptRoot\Show-VM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Show-VM" {
	Context "Shows VM" {
		$script:TestFlag = $false
		$FakeVM = New-FakeVM -OpenLocalGUI {
			$script:TestFlag = $true
		}	

		It "Show-VM calls method on vm." {
			$FakeVM | Show-VM
			$script:TestFlag | should be $true
		}
	}
}