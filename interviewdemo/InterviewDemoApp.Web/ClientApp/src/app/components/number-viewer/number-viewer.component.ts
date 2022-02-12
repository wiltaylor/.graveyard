import {Component, OnInit} from '@angular/core';
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
  numberToAdd = 1;

  constructor(private numberManagerService: NumberManagerService) { }

  ngOnInit(): void {
    this.numberManagerService.onMessage().subscribe(message => {
      this.serverText += message + '\n';
    });
  }

  addNumber(){
    this.numberManagerService.addNumber(this.numberToAdd).subscribe();
  }

  halt(){
    this.numberManagerService.halt().subscribe();
  }

  resume(){
    this.numberManagerService.resume().subscribe();
  }

  quit(){
    this.numberManagerService.quit().subscribe();
  }

  setInterval(){
    this.hideIntervalControls = true;
    this.numberManagerService.connect(this.interval).subscribe();
  }

}
