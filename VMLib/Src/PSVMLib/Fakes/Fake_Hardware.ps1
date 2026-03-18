class VMCDRom {
	[string]$Type;
	[string]$Path;
	[HashTable]$CustomSettings
}

class VMFloppyDisk {
	[string]$Type;
	[string]$Path;
	[HashTable]$CustomSettings
}

class VMHardDisk {
	[string]$Type;
	[string]$Path;
	[HashTable]$CustomSettings
}

class VMNetwork {
	[string]$Type;
	[string]$MACAddress;
	[string]$IsolatedNetworkName
	[HashTable]$CustomSettings
}

class VMCreationInfo {
		[string] $Path;
		[string] $Name;
		[string] $GuestOS;
		[int] $Memory;
		[int] $CPU;
		[int] $Cores;
		[HashTable] $CustomSettings;
		[object] $Disks;
		[object] $Networks;
}