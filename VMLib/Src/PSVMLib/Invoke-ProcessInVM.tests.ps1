.  $PSScriptRoot\Invoke-ProcessInVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Invoke-ProcessInVM" {
	Context "Can execute process on virtual machine" {
		$script:TestFlag = $false
		$fakeVM = New-FakeVM -ExecuteCommand {
			param($path, $args, $wait, $interactive)

			if($path -eq "C:\myapp.exe" -and $args -eq "myargs"){
				$script:TestFlag = $true
			}

		}
		
		It "can execute procecss" {
			$fakeVM | Invoke-ProcessInVM -Path "c:\myapp.exe" -Arguments "myargs" -Username "MyUsername" -Password "MyPassword"
		}

		It "Credentials are set on vm" {
			$fakeVM.Username | should be "MyUsername"
			$fakeVM.Password | should be "MyPassword"
		}
	}
}