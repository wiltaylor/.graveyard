import {Component, Input, OnInit, ViewChild} from '@angular/core';
import {NumberManagerService} from "../../services/number-manager.service";

@Component({
  selector: 'app-number-viewer',
  templateUrl: './number-viewer.component.html',
  styleUrls: ['./number-viewer.component.css']
})
export class NumberViewerComponent implements OnInit {

  interval: number = 1;
  hideIntervalControls = false;
  serverText = "";

  constructor(private numberManagerService: NumberManagerService) { }

  ngOnInit(): void {
  }

  setInterval(){
    this.hideIntervalControls = true;
    this.numberManagerService.connect(this.interval).subscribe(() => {
      this.numberManagerService.onMessage().subscribe(message => {
        this.serverText += message + '\n';
      });
    })

  }

}
