.  $PSScriptRoot\Restart-VM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Restart-VM" {
	Context "Can restart virtual machine" {
		$script:TestFlag = $false
		$fakevm = New-FakeVM -Restart {
			param($force)

			$script:TestFlag = $true

		}
	
		It "Restarts vm" {
			$fakevm | Restart-VM
		}
	}
}