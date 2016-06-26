.  $PSScriptRoot\Read-VMSetting.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Read-VMSetting" {
	Context "Can read vm settings" {
		$script:EnvironmentFlag = $false
		$script:GuestVMtFlag = $false
		$script:VMSettingFlag = $false
	
		$fakevm = new-FakeVM -ReadEnvironment {
			param($name)

			if($name -eq "MyEnvironment") { $script:EnvironmentFlag = $true }

		} -ReadGuestVariable {
			param($name)

			if($name -eq "MyGuest") { $script:GuestVMtFlag = $true }
		} -ReadVMSetting {
			param($name)

			if($name -eq "MyVMSetting") { $script:VMSettingFlag = $true }
		}

		It "Can get Environment variable" {
			$fakevm | Read-VMSetting -Name "MyEnvironment" -Type Environment
			$script:EnvironmentFlag | should be $true
		}

		It "Can get GuestVM Setting" {
			$fakevm | Read-VMSetting -Name "MyGuest" -Type GuestSetting 
			$script:GuestVMtFlag | should be $true
		}

		It "Can get VMsetting" { 
			$fakevm | Read-VMSetting -Name "MyVMSetting" -Type VMSetting
			$script:VMSettingFlag | should be $true
		}
	}
}