import {Component, Input, OnInit, ViewChild} from '@angular/core';

@Component({
  selector: 'app-number-viewer',
  templateUrl: './number-viewer.component.html',
  styleUrls: ['./number-viewer.component.css']
})
export class NumberViewerComponent implements OnInit {

  interval: number = 1;
  hideIntervalControls = false;

  constructor() { }

  ngOnInit(): void {
  }

  setInterval(){
    this.hideIntervalControls = true;
  }

}
