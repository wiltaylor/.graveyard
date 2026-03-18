.  $PSScriptRoot\Get-VMSnapshot.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Get-VMSnapshot" {
	Context "Gets list of snapshots on VM." {
		$FakeVM = New-FakeVM -Snapshots	@("MySnapshot", "SomeOthersnapshot")

		It "Returns list of snapshots" {
			$snapshots = $FakeVM | Get-VMSnapshot
			$snapshots[0] | should be "MySnapshot"
		}
	}
} 