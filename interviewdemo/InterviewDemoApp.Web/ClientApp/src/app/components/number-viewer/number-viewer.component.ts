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
  disableControls = false;

  constructor(private numberManagerService: NumberManagerService) { }

  ngOnInit(): void {
    this.numberManagerService.onMessage().subscribe(message => {
      this.serverText += message + '\n';
    });
  }

  // addNumber - Click handler for add number button.
  //             this will add the current entered number to the number manager service.
  addNumber(){
    this.numberManagerService.addNumber(this.numberToAdd).subscribe();
  }

  // halt - Click handler for halt button.
  //        this will instruct the number manager service to halt sending updates.
  halt(){
    this.numberManagerService.halt().subscribe();
  }

  // resume - Click handler for resume button.
  //          this will instruct the number manager service to resume sending updates again.
  resume(){
    this.numberManagerService.resume().subscribe();
  }

  // quit - Click handler for the quit button.
  //        this will instruct the number manager service to quit and drop the connection.
  quit(){
    this.numberManagerService.quit().subscribe();
    this.disableControls = true;
  }

  // setInterval - click handler for the set interval button.
  //               this will set the interval for updates and allow the user to enter numbers.
  setInterval(){
    this.hideIntervalControls = true;
    this.numberManagerService.connect(this.interval).subscribe();
  }

}
