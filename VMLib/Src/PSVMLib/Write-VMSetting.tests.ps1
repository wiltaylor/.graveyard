.  $PSScriptRoot\Write-VMSetting.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1


Describe "Write-VMSetting" {
	Context "Can write virtual machine settings" {
		$script:EnvironmentSetting = $null
		$script:GuestVMtSetting = $null
		$script:VMSettingSetting = $null
	
		$fakevm = new-FakeVM -WriteEnvironment {
			param($name, $value)

			$script:EnvironmentSetting = $value

		} -WriteGuestVariable {
			param($name, $value)

			$script:GuestVMtSetting = $value
		} -WriteVMSetting {
			param($name, $value)

			$script:VMSettingSetting = $value
		}

		It "Can set Environment variable" {
			$fakevm | Write-VMSetting -Name "MySetting" -Value "MyEnvironmentValue" -Type Environment
			$script:EnvironmentSetting | should be "MyEnvironmentValue"
		}

		It "Can set GuestVM Setting" {
			$fakevm | Write-VMSetting -Name "MySetting" -Value "GuestVMtSetting" -Type GuestSetting 
			$script:GuestVMtSetting | should be "GuestVMtSetting"
		}

		It "Can set VMsetting" { 
			$fakevm | Write-VMSetting -Name "MySetting" -Value "VMSettingSetting" -Type VMSetting
			$script:VMSettingSetting | should be "VMSettingSetting"
		}
	}
}